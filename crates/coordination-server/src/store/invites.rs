use anyhow::{Context, Result};
use sqlx::Row;
use time::OffsetDateTime;

use crate::util::format_rfc3339;

use super::{Db, normalize_room_id};

#[derive(Debug, Clone)]
pub struct LinkInviteRecord {
    pub invite_id: String,
    pub room_id: String,
    pub expires_at: String,
    pub revoked_at: Option<String>,
}

impl Db {
    pub async fn create_link_invite(
        &self,
        room_id: &str,
        created_by_workos_user_id: &str,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<LinkInviteRecord> {
        let invite_id = format!("invlink_{}", uuid::Uuid::new_v4().simple());
        let row = sqlx::query(
            "INSERT INTO room_invite_links (
                invite_id,
                room_id,
                token_hash,
                created_by_workos_user_id,
                expires_at
             )
             VALUES ($1, $2, $3, $4, $5)
             RETURNING revoked_at, created_at",
        )
        .bind(&invite_id)
        .bind(normalize_room_id(room_id))
        .bind(token_hash)
        .bind(created_by_workos_user_id)
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await
        .with_context(|| format!("failed to create invite link for room {room_id}"))?;

        let revoked_at: Option<OffsetDateTime> = row
            .try_get("revoked_at")
            .context("failed to decode invite revoked_at")?;

        Ok(LinkInviteRecord {
            invite_id,
            room_id: normalize_room_id(room_id),
            expires_at: format_rfc3339(expires_at),
            revoked_at: revoked_at.map(format_rfc3339),
        })
    }

    pub async fn get_link_invite(&self, token_hash: &str) -> Result<Option<LinkInviteRecord>> {
        let row = sqlx::query(
            "SELECT invite_id, room_id, created_by_workos_user_id, expires_at, revoked_at
             FROM room_invite_links
             WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .with_context(|| "failed to fetch invite link".to_owned())?;

        row.map(|row| {
            let expires_at: OffsetDateTime = row.try_get("expires_at")?;
            let revoked_at: Option<OffsetDateTime> = row.try_get("revoked_at")?;
            Ok(LinkInviteRecord {
                invite_id: row.try_get("invite_id")?,
                room_id: row.try_get("room_id")?,
                expires_at: format_rfc3339(expires_at),
                revoked_at: revoked_at.map(format_rfc3339),
            })
        })
        .transpose()
    }
}
