pub mod config;
pub(crate) mod jwt;
pub(crate) mod workos;

use axum::http::{HeaderMap, StatusCode, header};
use reporter_protocol::Room;

use crate::state::AppState;
use crate::store::RoomRecord;
use crate::util::internal_error;

pub use config::AuthConfig;

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedUser {
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RoomMembership {
    pub room_id: String,
    pub role: String,
}

pub(crate) async fn require_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, (StatusCode, String)> {
    let token =
        bearer_token(headers).ok_or((StatusCode::UNAUTHORIZED, "sign in required".to_owned()))?;
    let claims = state
        .auth
        .verify_access_token(token)
        .await
        .map_err(unauthorized_error)?;
    Ok(AuthenticatedUser {
        user_id: claims.sub,
    })
}

pub(crate) async fn create_room_for_owner(
    state: &AppState,
    name: &str,
    owner_user_id: &str,
) -> Result<Room, (StatusCode, String)> {
    let name = name.trim();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "room name is required".to_owned()));
    }

    let organization = state
        .auth
        .create_room_organization(name)
        .await
        .map_err(internal_error)?;
    state
        .auth
        .ensure_membership(owner_user_id, &organization.id)
        .await
        .map_err(internal_error)?;
    state
        .db
        .create_room(name, &organization.id, owner_user_id)
        .await
        .map_err(internal_error)
}

pub(crate) async fn ensure_room_member(
    state: &AppState,
    headers: &HeaderMap,
    room_id: &str,
) -> Result<(AuthenticatedUser, RoomMembership), (StatusCode, String)> {
    let user = require_user(state, headers).await?;
    let room_record = state
        .db
        .get_room(room_id)
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))?;

    let memberships = state
        .auth
        .list_memberships(&user.user_id, &room_record.workos_organization_id)
        .await
        .map_err(internal_error)?;

    if memberships.is_empty() {
        return Err((StatusCode::FORBIDDEN, "room access denied".to_owned()));
    }

    let role = viewer_role(&room_record, &user.user_id);
    Ok((
        user.clone(),
        RoomMembership {
            room_id: room_record.room_id,
            role,
        },
    ))
}

pub(crate) fn viewer_role(room_record: &RoomRecord, user_id: &str) -> String {
    if room_record.owner_workos_user_id == user_id {
        "owner".to_owned()
    } else {
        "member".to_owned()
    }
}

pub(crate) fn require_owner(membership: &RoomMembership) -> Result<(), (StatusCode, String)> {
    if membership.role == "owner" {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "only room owners can manage invites".to_owned(),
        ))
    }
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}

fn unauthorized_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::UNAUTHORIZED, error.to_string())
}
