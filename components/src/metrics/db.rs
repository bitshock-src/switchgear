use opentelemetry_semantic_conventions::attribute::{
    DB_COLLECTION_NAME, DB_NAMESPACE, DB_RESPONSE_STATUS_CODE, DB_SYSTEM_NAME, ERROR_TYPE,
    SERVER_ADDRESS, SERVER_PORT,
};
use sea_orm::{
    DatabaseConnection, DatabaseConnectionType, DbBackend, DbErr, RuntimeErr, SqlErr,
    TransactionError,
};
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use std::path::Path;
use std::time::Duration;

const ERROR_TYPE_OTHER: &str = "_OTHER";
const SWGR_OPERATION: &str = "swgr.operation";

const SQLITE: &str = "sqlite";
const MYSQL: &str = "mysql";
const POSTGRESQL: &str = "postgresql";
const SQLX_IN_MEMORY_PREFIX: &str = "file:sqlx-in-memory-";

#[derive(Clone, Debug)]
pub(crate) struct DbTarget {
    pub system: &'static str,
    pub namespace: Option<String>,
    pub address: Option<String>,
    pub port: Option<u16>,
}

pub(crate) fn db_target(db: &DatabaseConnection) -> DbTarget {
    match &db.inner {
        DatabaseConnectionType::SqlxPostgresPoolConnection(_) => {
            postgres_target(&db.get_postgres_connection_pool().connect_options())
        }
        DatabaseConnectionType::SqlxMySqlPoolConnection(_) => {
            mysql_target(&db.get_mysql_connection_pool().connect_options())
        }
        DatabaseConnectionType::SqlxSqlitePoolConnection(_) => {
            sqlite_target(&db.get_sqlite_connection_pool().connect_options())
        }
        _ => DbTarget {
            system: unsupported_system(db.get_database_backend()),
            namespace: None,
            address: None,
            port: None,
        },
    }
}

fn unsupported_system(backend: DbBackend) -> &'static str {
    match backend {
        DbBackend::Sqlite => SQLITE,
        DbBackend::MySql => MYSQL,
        DbBackend::Postgres => POSTGRESQL,
        _ => ERROR_TYPE_OTHER,
    }
}

fn postgres_target(options: &PgConnectOptions) -> DbTarget {
    DbTarget {
        system: POSTGRESQL,
        namespace: options.get_database().map(str::to_owned),
        address: non_empty(options.get_host()),
        port: Some(options.get_port()),
    }
}

fn mysql_target(options: &MySqlConnectOptions) -> DbTarget {
    DbTarget {
        system: MYSQL,
        namespace: options.get_database().map(str::to_owned),
        address: non_empty(options.get_host()),
        port: Some(options.get_port()),
    }
}

fn sqlite_target(options: &SqliteConnectOptions) -> DbTarget {
    DbTarget {
        system: SQLITE,
        namespace: sqlite_namespace(options.get_filename()),
        address: None,
        port: None,
    }
}

fn sqlite_namespace(filename: &Path) -> Option<String> {
    let filename = filename.to_string_lossy();
    if filename == ":memory:" || filename.starts_with(SQLX_IN_MEMORY_PREFIX) {
        return Some(":memory:".to_owned());
    }
    Path::new(filename.as_ref())
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

pub(crate) trait DbOutcome {
    fn error_type(&self) -> &'static str;
    fn status_code(&self) -> Option<String>;
}

impl DbOutcome for DbErr {
    fn error_type(&self) -> &'static str {
        if let Some(sql_err) = self.sql_err() {
            return match sql_err {
                SqlErr::UniqueConstraintViolation(_) => "unique_constraint",
                SqlErr::ForeignKeyConstraintViolation(_) => "foreign_key_constraint",
                _ => "constraint",
            };
        }
        match self {
            DbErr::ConnectionAcquire(_) => "connection_acquire",
            DbErr::Conn(_) => "connection",
            DbErr::Query(_) | DbErr::Exec(_) => "statement",
            DbErr::Type(_) | DbErr::Json(_) | DbErr::TryIntoErr { .. } => "conversion",
            _ => ERROR_TYPE_OTHER,
        }
    }

    fn status_code(&self) -> Option<String> {
        let runtime = match self {
            DbErr::Conn(e) | DbErr::Exec(e) | DbErr::Query(e) => e,
            _ => return None,
        };
        let RuntimeErr::SqlxError(e) = runtime else {
            return None;
        };
        match e.as_ref() {
            sqlx::Error::Database(db) => db.code().map(|code| code.into_owned()),
            _ => None,
        }
    }
}

impl DbOutcome for TransactionError<DbErr> {
    fn error_type(&self) -> &'static str {
        match self {
            TransactionError::Connection(e) | TransactionError::Transaction(e) => e.error_type(),
        }
    }

    fn status_code(&self) -> Option<String> {
        match self {
            TransactionError::Connection(e) | TransactionError::Transaction(e) => e.status_code(),
        }
    }
}

pub(crate) fn record_db_operation<E: DbOutcome>(
    elapsed: Duration,
    target: &DbTarget,
    collection: &'static str,
    operation: &'static str,
    result: &Result<impl Sized, E>,
) {
    let error = result.as_ref().err();
    let status_code = error.and_then(|e| e.status_code());
    switchgear_metrics::histogram!(
        "db.client.operation.duration",
        elapsed,
        DB_SYSTEM_NAME => target.system,
        DB_NAMESPACE => target.namespace.as_deref(),
        SERVER_ADDRESS => target.address.as_deref(),
        SERVER_PORT => target.port,
        DB_COLLECTION_NAME => collection,
        SWGR_OPERATION => operation,
        ERROR_TYPE => error.map(|e| e.error_type()),
        DB_RESPONSE_STATUS_CODE => status_code.as_deref(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pg(url: &str) -> PgConnectOptions {
        url.parse().expect("postgres options")
    }

    fn my(url: &str) -> MySqlConnectOptions {
        url.parse().expect("mysql options")
    }

    fn lite(url: &str) -> SqliteConnectOptions {
        url.parse().expect("sqlite options")
    }

    #[test]
    fn postgres_target_reads_the_driver_options() {
        let target = postgres_target(&pg("postgres://user:pw@db.example:6543/offers"));

        assert_eq!(target.system, POSTGRESQL);
        assert_eq!(target.address.as_deref(), Some("db.example"));
        assert_eq!(target.port, Some(6543));
        assert_eq!(target.namespace.as_deref(), Some("offers"));
    }

    #[test]
    fn mysql_target_reads_the_driver_options() {
        let target = mysql_target(&my("mysql://user:pw@db.example:3307/offers"));

        assert_eq!(target.system, MYSQL);
        assert_eq!(target.address.as_deref(), Some("db.example"));
        assert_eq!(target.port, Some(3307));
        assert_eq!(target.namespace.as_deref(), Some("offers"));
    }

    #[test]
    fn a_target_carries_no_credentials() {
        let rendered = format!(
            "{:?} {:?}",
            postgres_target(&pg("postgres://hunter2user:hunter2pass@h/db")),
            mysql_target(&my("mysql://hunter2user:hunter2pass@h/db")),
        );

        assert!(!rendered.contains("hunter2"), "{rendered}");
    }

    #[test]
    fn sqlite_target_reads_the_file_stem() {
        let target = sqlite_target(&lite("sqlite:///data/offer_store.db?mode=rwc"));

        assert_eq!(target.system, SQLITE);
        assert_eq!(target.namespace.as_deref(), Some("offer_store"));
        assert_eq!(target.address, None);
        assert_eq!(target.port, None);
    }

    #[test]
    fn sqlite_target_names_an_in_memory_database() {
        let target = sqlite_target(&lite("sqlite::memory:"));

        assert_eq!(target.namespace.as_deref(), Some(":memory:"));
        assert_eq!(target.address, None);
        assert_eq!(target.port, None);
    }

    #[tokio::test]
    async fn db_target_reads_a_live_connection() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("offer_store.db");
        let db =
            sea_orm::Database::connect(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
                .await
                .expect("connect");

        let target = db_target(&db);

        assert_eq!(target.system, SQLITE);
        assert_eq!(target.namespace.as_deref(), Some("offer_store"));
        assert_eq!(target.address, None);
        assert_eq!(target.port, None);
    }

    #[test]
    fn db_outcome_classifies_each_db_err_variant() {
        let cases: Vec<(DbErr, &str)> = vec![
            (
                DbErr::ConnectionAcquire(sea_orm::ConnAcquireErr::Timeout),
                "connection_acquire",
            ),
            (
                DbErr::ConnectionAcquire(sea_orm::ConnAcquireErr::ConnectionClosed),
                "connection_acquire",
            ),
            (DbErr::Conn(RuntimeErr::Internal("x".into())), "connection"),
            (DbErr::Query(RuntimeErr::Internal("x".into())), "statement"),
            (DbErr::Exec(RuntimeErr::Internal("x".into())), "statement"),
            (DbErr::Type("x".into()), "conversion"),
            (DbErr::Json("x".into()), "conversion"),
            (
                DbErr::TryIntoErr {
                    from: "a",
                    into: "b",
                    source: std::sync::Arc::new(std::fmt::Error),
                },
                "conversion",
            ),
        ];

        for (err, expected) in cases {
            assert_eq!(err.error_type(), expected, "{err:?}");
            assert_eq!(
                TransactionError::Transaction(err.clone()).error_type(),
                expected,
                "{err:?}"
            );
            assert_eq!(
                TransactionError::Connection(err.clone()).error_type(),
                expected,
                "{err:?}"
            );
        }
    }

    #[test]
    fn db_outcome_falls_back_to_the_registry_value() {
        for err in [
            DbErr::RecordNotFound("x".into()),
            DbErr::RecordNotInserted,
            DbErr::RecordNotUpdated,
            DbErr::Custom("x".into()),
            DbErr::Migration("x".into()),
            DbErr::UnpackInsertId,
            DbErr::MutexPoisonError,
            DbErr::AccessDenied {
                permission: "x".into(),
                resource: "y".into(),
            },
        ] {
            assert_eq!(err.error_type(), "_OTHER", "{err:?}");
        }
    }

    #[test]
    fn db_outcome_has_no_status_code_without_a_driver_error() {
        assert_eq!(
            DbErr::Query(RuntimeErr::Internal("x".into())).status_code(),
            None
        );
        assert_eq!(DbErr::RecordNotInserted.status_code(), None);
    }

    #[tokio::test]
    async fn db_outcome_extracts_a_driver_status_code() {
        use sqlx::Connection;

        let mut conn = sqlx::SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("connect");
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&mut conn)
            .await
            .expect("create");
        sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(&mut conn)
            .await
            .expect("first insert");
        let sqlx_err = sqlx::query("INSERT INTO t (id) VALUES (1)")
            .execute(&mut conn)
            .await
            .expect_err("unique violation");

        let err = DbErr::Exec(RuntimeErr::SqlxError(std::sync::Arc::new(sqlx_err)));

        assert_eq!(err.error_type(), "unique_constraint");
        assert_eq!(
            foreign_key_error(&mut conn).await.error_type(),
            "foreign_key_constraint"
        );
        let code = err.status_code().expect("driver code");
        assert!(code.parse::<i64>().is_ok(), "{code}");
        assert_eq!(
            TransactionError::Transaction(err).status_code().as_deref(),
            Some(code.as_str())
        );
    }

    async fn foreign_key_error(conn: &mut sqlx::SqliteConnection) -> DbErr {
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *conn)
            .await
            .expect("pragma");
        sqlx::query("CREATE TABLE child (id INTEGER PRIMARY KEY, parent INTEGER REFERENCES t(id))")
            .execute(&mut *conn)
            .await
            .expect("create child");
        let err = sqlx::query("INSERT INTO child (id, parent) VALUES (1, 999)")
            .execute(&mut *conn)
            .await
            .expect_err("foreign key violation");

        DbErr::Exec(RuntimeErr::SqlxError(std::sync::Arc::new(err)))
    }
}
