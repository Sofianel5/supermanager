mod events;
mod invites;
mod rooms;
mod summaries;
#[cfg(test)]
pub(crate) mod test_support;

use anyhow::{Context, Result};
use rand::Rng;
use sqlx::{
    PgPool,
    migrate::Migrator,
    postgres::PgPoolOptions,
};

pub use invites::LinkInviteRecord;
pub use rooms::RoomRecord;

const ROOM_CODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ROOM_CODE_LENGTH: usize = 6;

static MIGRATOR: Migrator = sqlx::migrate!();

#[derive(Clone)]
pub struct Db {
    pub(crate) pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("failed to connect to PostgreSQL")?;

        MIGRATOR
            .run(&pool)
            .await
            .context("failed to run PostgreSQL migrations")?;

        Ok(Self { pool })
    }

    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("failed database health check")?;
        Ok(())
    }
}

pub fn normalize_room_id(room_id: &str) -> String {
    room_id.trim().to_ascii_uppercase()
}

pub(crate) fn generate_room_code(rng: &mut impl Rng) -> String {
    (0..ROOM_CODE_LENGTH)
        .map(|_| {
            let index = rng.random_range(0..ROOM_CODE_ALPHABET.len());
            ROOM_CODE_ALPHABET[index] as char
        })
        .collect()
}

pub(crate) fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(db_error)
            if db_error.code().as_deref() == Some("23505")
    )
}
