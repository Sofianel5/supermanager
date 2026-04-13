use anyhow::{Context, Result};
use reporter_protocol::Room;
use sqlx::Row;
use time::OffsetDateTime;

use crate::util::format_rfc3339;

use super::{Db, generate_room_code, is_unique_violation, normalize_room_id};

#[derive(Debug, Clone)]
pub struct RoomRecord {
    pub room_id: String,
    pub name: String,
    pub created_at: String,
    pub workos_organization_id: String,
    pub owner_workos_user_id: String,
}

impl Db {
    pub async fn create_room(
        &self,
        name: &str,
        workos_organization_id: &str,
        owner_workos_user_id: &str,
    ) -> Result<Room> {
        for _ in 0..10 {
            let room_id = {
                let mut rng = rand::rng();
                generate_room_code(&mut rng)
            };

            let insert = sqlx::query(
                "INSERT INTO rooms (room_id, name, workos_organization_id, owner_workos_user_id)
                 VALUES ($1, $2, $3, $4)
                 RETURNING created_at",
            )
            .bind(&room_id)
            .bind(name)
            .bind(workos_organization_id)
            .bind(owner_workos_user_id)
            .fetch_one(&self.pool)
            .await;

            match insert {
                Ok(row) => {
                    let created_at: OffsetDateTime = row
                        .try_get("created_at")
                        .context("failed to decode room created_at")?;
                    return Ok(Room {
                        room_id,
                        name: name.to_owned(),
                        created_at: format_rfc3339(created_at),
                    });
                }
                Err(error) if is_unique_violation(&error) => continue,
                Err(error) => {
                    return Err(error).context("failed to insert room into PostgreSQL");
                }
            }
        }

        anyhow::bail!("failed to generate unique room code after 10 attempts")
    }

    pub async fn get_room(&self, room_id: &str) -> Result<Option<RoomRecord>> {
        let row = sqlx::query(
            "SELECT room_id, name, created_at, workos_organization_id, owner_workos_user_id
             FROM rooms
             WHERE room_id = $1",
        )
        .bind(normalize_room_id(room_id))
        .fetch_optional(&self.pool)
        .await
        .with_context(|| format!("failed to fetch room {room_id}"))?;

        row.map(|row| {
            let created_at: OffsetDateTime = row.try_get("created_at")?;
            Ok(RoomRecord {
                room_id: row.try_get("room_id")?,
                name: row.try_get("name")?,
                created_at: format_rfc3339(created_at),
                workos_organization_id: row.try_get("workos_organization_id")?,
                owner_workos_user_id: row.try_get("owner_workos_user_id")?,
            })
        })
        .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::test_support::TestDb;

    async fn create_test_room(db: &Db, name: &str) -> Room {
        db.create_room(name, "org_test", "user_test").await.unwrap()
    }

    #[tokio::test]
    async fn room_lookup_normalizes_case() {
        let Some(test_db) = TestDb::new().await else {
            eprintln!("skipping PostgreSQL test: TEST_DATABASE_URL is not set");
            return;
        };

        let room = create_test_room(&test_db.db, "Case Test").await;
        let fetched = test_db
            .db
            .get_room(&room.room_id.to_ascii_lowercase())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(fetched.room_id, room.room_id);
        test_db.cleanup().await;
    }
}
