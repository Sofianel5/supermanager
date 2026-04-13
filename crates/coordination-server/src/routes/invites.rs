use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use rand::distr::{Alphanumeric, SampleString};
use reporter_protocol::{
    AcceptInviteRequest, AcceptInviteResponse, AuthConfigResponse, CliRefreshRequest,
    CliRefreshResponse, CreateInviteRequest, CurrentUserResponse, InviteResponse,
    RoomMetadataResponse,
};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};

use crate::auth::{self, viewer_role};
use crate::state::AppState;
use crate::util::{internal_error, trim_url};

const LINK_TOKEN_LENGTH: usize = 48;

pub async fn auth_config(State(state): State<AppState>) -> Json<AuthConfigResponse> {
    Json(AuthConfigResponse {
        client_id: state.auth.client_id().to_owned(),
    })
}

pub async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CurrentUserResponse>, (StatusCode, String)> {
    let user = auth::require_user(&state, &headers).await?;
    let profile = state
        .auth
        .get_user(&user.user_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(auth::workos::map_current_user(profile)))
}

pub async fn refresh_cli_token(
    State(state): State<AppState>,
    Json(request): Json<CliRefreshRequest>,
) -> Result<Json<CliRefreshResponse>, (StatusCode, String)> {
    if request.refresh_token.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "refresh token is required".to_owned(),
        ));
    }

    let response = state
        .auth
        .refresh_cli_session(&request.refresh_token)
        .await
        .map_err(internal_error)?;
    Ok(Json(response))
}

pub async fn create_link_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<InviteResponse>, (StatusCode, String)> {
    let (user, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    auth::require_owner(&membership)?;

    let token = generate_link_token();
    let expires_at = OffsetDateTime::now_utc() + Duration::days(state.auth.link_invite_days);
    let invite = state
        .db
        .create_link_invite(
            &membership.room_id,
            &user.user_id,
            &hash_token(&token),
            expires_at,
        )
        .await
        .map_err(internal_error)?;

    Ok(Json(InviteResponse {
        invite_id: invite.invite_id,
        room_id: invite.room_id,
        kind: "link".to_owned(),
        invite_url: format!("{}/invite/{}", trim_url(&state.public_app_url), token),
        target_email: None,
        expires_at: invite.expires_at,
    }))
}

pub async fn create_email_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(request): Json<CreateInviteRequest>,
) -> Result<Json<InviteResponse>, (StatusCode, String)> {
    let target_email = request
        .target_email
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "target email is required".to_owned(),
        ))?;

    let (user, membership) = auth::ensure_room_member(&state, &headers, &room_id).await?;
    auth::require_owner(&membership)?;

    let room_record = state
        .db
        .get_room(&membership.room_id)
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))?;

    let invitation = state
        .auth
        .create_email_invitation(
            &room_record.workos_organization_id,
            &user.user_id,
            target_email,
        )
        .await
        .map_err(internal_error)?;

    Ok(Json(InviteResponse {
        invite_id: invitation.id,
        room_id: membership.room_id,
        kind: "email".to_owned(),
        invite_url: invitation.accept_invitation_url,
        target_email: Some(invitation.email),
        expires_at: invitation.expires_at,
    }))
}

pub async fn accept_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AcceptInviteRequest>,
) -> Result<Json<AcceptInviteResponse>, (StatusCode, String)> {
    let user = auth::require_user(&state, &headers).await?;
    let invite = state
        .db
        .get_link_invite(&hash_token(&request.token))
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "invite not found".to_owned()))?;

    enforce_link_invite_valid(&invite)?;

    let room_record = state
        .db
        .get_room(&invite.room_id)
        .await
        .map_err(internal_error)?
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("room not found: {}", invite.room_id),
        ))?;

    state
        .auth
        .ensure_membership(&user.user_id, &room_record.workos_organization_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(AcceptInviteResponse {
        room: RoomMetadataResponse {
            room_id: room_record.room_id.clone(),
            name: room_record.name.clone(),
            created_at: room_record.created_at.clone(),
            viewer_role: viewer_role(&room_record, &user.user_id),
        },
    }))
}

fn generate_link_token() -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, LINK_TOKEN_LENGTH)
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn enforce_link_invite_valid(
    invite: &crate::store::LinkInviteRecord,
) -> Result<(), (StatusCode, String)> {
    if invite.revoked_at.is_some() {
        return Err((StatusCode::GONE, "invite has been revoked".to_owned()));
    }

    let expires_at = auth::workos::parse_rfc3339(&invite.expires_at)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    if expires_at <= OffsetDateTime::now_utc() {
        return Err((StatusCode::GONE, "invite has expired".to_owned()));
    }

    Ok(())
}
