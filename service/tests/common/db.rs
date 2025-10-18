use std::net::SocketAddr;
use std::thread;

pub struct TestMysqlDatabase {
    db_name: String,
    connection_url: String,
    addr: SocketAddr,
}

impl TestMysqlDatabase {
    // 3306
    pub fn new(db_name: String, addr: SocketAddr) -> Self {
        let db_name_clone = db_name.clone();
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

                let _ = sqlx::query(&format!("CREATE DATABASE {db_name_clone}"))
                    .execute(&pool)
                    .await;
            });
        })
        .join();

        let connection_url = format!("mysql://root:mysql@{addr}/{db_name}");
        Self {
            db_name,
            connection_url,
            addr,
        }
    }

    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }
}

impl Drop for TestMysqlDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let addr = self.addr;
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
    addr: SocketAddr,
}

impl TestPostgresDatabase {
    pub fn new(db_name: String, addr: SocketAddr) -> Self {
        let db_name_clone = db_name.clone();
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

                let _ = sqlx::query(&format!("CREATE DATABASE {db_name_clone}"))
                    .execute(&pool)
                    .await;
            });
        })
        .join();

        let connection_url = format!("postgres://postgres:postgres@{addr}/{db_name}");
        Self {
            db_name,
            connection_url,
            addr,
        }
    }

    pub fn connection_url(&self) -> &str {
        &self.connection_url
    }
}

impl Drop for TestPostgresDatabase {
    fn drop(&mut self) {
        let db_name = self.db_name.clone();
        let addr = self.addr;
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
