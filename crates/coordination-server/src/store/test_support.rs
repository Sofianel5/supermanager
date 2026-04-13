use std::str::FromStr;

use anyhow::Result;
use sqlx::{Connection, PgConnection};
use url::Url;
use uuid::Uuid;

use super::Db;

pub(crate) struct TestDb {
    pub(crate) db: Db,
    admin_database_url: String,
    database_name: String,
}

impl TestDb {
    pub(crate) async fn new() -> Option<Self> {
        let admin_database_url = std::env::var("TEST_DATABASE_URL")
            .ok()
            .or_else(|| std::env::var("DATABASE_URL").ok())?;

        let database_name = format!("supermanager_test_{}", Uuid::new_v4().simple());
        let mut admin = PgConnection::connect(&admin_database_url).await.unwrap();
        sqlx::query(&format!(r#"CREATE DATABASE "{database_name}""#))
            .execute(&mut admin)
            .await
            .unwrap();
        drop(admin);

        let database_url = database_url_for_test(&admin_database_url, &database_name).unwrap();
        let db = Db::connect(&database_url).await.unwrap();

        Some(Self {
            db,
            admin_database_url,
            database_name,
        })
    }

    pub(crate) async fn cleanup(self) {
        self.db.pool.close().await;

        let mut admin = PgConnection::connect(&self.admin_database_url)
            .await
            .unwrap();
        sqlx::query(
            "SELECT pg_terminate_backend(pid)
             FROM pg_stat_activity
             WHERE datname = $1
               AND pid <> pg_backend_pid()",
        )
        .bind(&self.database_name)
        .execute(&mut admin)
        .await
        .unwrap();
        sqlx::query(&format!(
            r#"DROP DATABASE IF EXISTS "{0}""#,
            self.database_name
        ))
        .execute(&mut admin)
        .await
        .unwrap();
    }
}

fn database_url_for_test(admin_database_url: &str, database_name: &str) -> Result<String> {
    let mut url = Url::from_str(admin_database_url)?;
    url.set_path(&format!("/{database_name}"));
    Ok(url.to_string())
}
