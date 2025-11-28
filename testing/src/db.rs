use std::path::Path;
use std::thread;

pub struct TestMysqlDatabase {
    db_name: String,
    connection_url: String,
    addr: String,
}

impl TestMysqlDatabase {
    pub fn new(db_name: String, addr: &str, ssl: bool, ssl_ca: Option<&Path>) -> Self {
        let addr_c = addr.to_string();
        let db_name_clone = db_name.clone();
        let _ = thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => return,
            };

            rt.block_on(async {
                use sqlx::mysql::MySqlPoolOptions;

                let pool = match MySqlPoolOptions::new()
                    .connect(&format!("mysql://root:mysql@{addr_c}/mysql"))
                    .await
                {
                    Ok(pool) => pool,
                    Err(_) => return,
                };

                let _ = sqlx::query(&format!("CREATE DATABASE {db_name_clone}"))
                    .execute(&pool)
                    .await;
            });
        })
        .join();

        let ssl = if ssl { "?ssl-mode=VERIFY_IDENTITY" } else { "" };

        let ssl_ca = match (!ssl.is_empty(), ssl_ca) {
            (true, Some(ssl_ca)) => format!("&ssl-ca={}", ssl_ca.to_string_lossy()),
            (_, _) => "".to_string(),
        };

        let connection_url = format!("mysql://root:mysql@{addr}/{db_name}{ssl}{ssl_ca}");
        Self {
            db_name,
            connection_url,
            addr: addr.to_string(),
        }
    }

    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }
}

impl Drop for TestMysqlDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let addr = self.addr.clone();
        let _ = thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => return,
            };

            rt.block_on(async {
                use sqlx::mysql::MySqlPoolOptions;

                let pool = match MySqlPoolOptions::new()
                    .connect(&format!("mysql://root:mysql@{addr}/mysql"))
                    .await
                {
                    Ok(pool) => pool,
                    Err(_) => return,
                };

                let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
                    .execute(&pool)
                    .await;
            });
        })
        .join();
    }
}

pub struct TestPostgresDatabase {
    db_name: String,
    connection_url: String,
    addr: String,
}

impl TestPostgresDatabase {
    pub fn new(db_name: String, addr: &str, ssl: bool, ssl_root_cert: Option<&Path>) -> Self {
        let db_name_clone = db_name.clone();
        let addr_c = addr.to_string();
        let _ = thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => return,
            };

            rt.block_on(async {
                use sqlx::postgres::PgPoolOptions;

                let pool = match PgPoolOptions::new()
                    .connect(&format!("postgres://postgres:postgres@{addr_c}/postgres"))
                    .await
                {
                    Ok(pool) => pool,
                    Err(_) => return,
                };

                let _ = sqlx::query(&format!("CREATE DATABASE {db_name_clone}"))
                    .execute(&pool)
                    .await;
            });
        })
        .join();

        let ssl = if ssl { "?sslmode=verify-full" } else { "" };

        let ssl_root_cert = match (!ssl.is_empty(), ssl_root_cert) {
            (true, Some(ssl_root_cert)) => {
                format!("&sslrootcert={}", ssl_root_cert.to_string_lossy())
            }
            (_, _) => "".to_string(),
        };

        let connection_url =
            format!("postgres://postgres:postgres@{addr}/{db_name}{ssl}{ssl_root_cert}");

        Self {
            db_name,
            connection_url,
            addr: addr.to_string(),
        }
    }

    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }
}

impl Drop for TestPostgresDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let addr = self.addr.clone();
        let _ = thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => return,
            };

            rt.block_on(async {
                use sqlx::postgres::PgPoolOptions;

                let pool = match PgPoolOptions::new()
                    .connect(&format!("postgres://postgres:postgres@{addr}/postgres"))
                    .await
                {
                    Ok(pool) => pool,
                    Err(_) => return,
                };

                let _ = sqlx::query(&format!("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{db_name}' AND pid <>  pg_backend_pid()"))
                    .execute(&pool).await;

                let _ = sqlx::query(&format!("DROP DATABASE IF EXISTS {db_name}"))
                    .execute(&pool)
                    .await;
            });
        })
            .join();
    }
}
