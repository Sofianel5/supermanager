use std::{collections::HashMap, env, sync::Arc};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use rand::distr::{Alphanumeric, SampleString};
use reporter_protocol::{
    AcceptInviteRequest, AcceptInviteResponse, AuthConfigResponse, CliRefreshRequest,
    CliRefreshResponse, CreateInviteRequest, CurrentUserResponse, InviteResponse, Room,
    RoomMetadataResponse,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use workos_client::{
    self,
    types::{
        AuthenticateResponse, CreateInvitationRequest, CreateOrganizationMembershipRequest,
        CreateOrganizationRequest, JwksResponse, User,
    },
};

use crate::store::{LinkInviteRecord, RoomAccessRecord};

use super::{AppState, internal_error, resolve_room, trim_url};

const DEFAULT_WORKOS_BASE_URL: &str = "https://api.workos.com";
const DEFAULT_MEMBER_ROLE: &str = "member";
const DEFAULT_EMAIL_INVITE_DAYS: i64 = 7;
const DEFAULT_LINK_INVITE_DAYS: i64 = 14;
const JWKS_CACHE_MINUTES: i64 = 60;
const LINK_TOKEN_LENGTH: usize = 48;

#[derive(Clone, Debug)]
pub struct AuthConfig {
    api_key: String,
    base_url: String,
    client_id: String,
    issuer: String,
    member_role_slug: String,
    email_invite_days: i64,
    link_invite_days: i64,
    client: workos_client::Client,
    jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
}

#[derive(Debug, Clone)]
struct CachedJwks {
    fetched_at: OffsetDateTime,
    keys: HashMap<String, CachedJwk>,
}

#[derive(Debug, Clone)]
struct CachedJwk {
    n: String,
    e: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthenticatedUser {
    pub user_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RoomMembership {
    pub room_id: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
struct AccessTokenClaims {
    sub: String,
    exp: usize,
}

#[derive(Debug, Serialize)]
struct RefreshSessionRequest<'a> {
    client_id: &'a str,
    grant_type: &'static str,
    refresh_token: &'a str,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = env::var("SUPERMANAGER_WORKOS_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_WORKOS_BASE_URL.to_owned());
        let client_id = required_env("SUPERMANAGER_WORKOS_CLIENT_ID")?;
        let api_key = required_env("SUPERMANAGER_WORKOS_API_KEY")?;
        let issuer = env::var("SUPERMANAGER_WORKOS_ISSUER")
            .unwrap_or_else(|_| base_url.trim_end_matches('/').to_owned());
        let member_role_slug = env::var("SUPERMANAGER_WORKOS_MEMBER_ROLE")
            .unwrap_or_else(|_| DEFAULT_MEMBER_ROLE.to_owned());
        let email_invite_days = optional_env_i64(
            "SUPERMANAGER_WORKOS_EMAIL_INVITE_DAYS",
            DEFAULT_EMAIL_INVITE_DAYS,
        )?;
        let link_invite_days = optional_env_i64(
            "SUPERMANAGER_WORKOS_LINK_INVITE_DAYS",
            DEFAULT_LINK_INVITE_DAYS,
        )?;

        let client = build_workos_client(&base_url, &api_key)?;

        Ok(Self {
            api_key,
            base_url,
            client_id,
            issuer,
            member_role_slug,
            email_invite_days,
            link_invite_days,
            client,
            jwks_cache: Arc::new(RwLock::new(None)),
        })
    }

    fn client_id(&self) -> &str {
        &self.client_id
    }

    async fn create_room_organization(
        &self,
        name: &str,
    ) -> Result<workos_client::types::Organization> {
        workos_ok(
            self.client
                .create_organization(&CreateOrganizationRequest {
                    external_id: None,
                    name: name.to_owned(),
                })
                .await,
        )
        .await
    }

    async fn get_user(&self, user_id: &str) -> Result<User> {
        workos_ok(self.client.get_user(user_id).await).await
    }

    async fn ensure_membership(&self, user_id: &str, organization_id: &str) -> Result<()> {
        let memberships = self.list_memberships(user_id, organization_id).await?;
        if memberships
            .iter()
            .any(|membership| membership.status == "active")
        {
            return Ok(());
        }

        workos_ok(
            self.client
                .create_organization_membership(&CreateOrganizationMembershipRequest {
                    organization_id: organization_id.to_owned(),
                    role_slug: Some(self.member_role_slug.clone()),
                    user_id: user_id.to_owned(),
                })
                .await,
        )
        .await?;
        Ok(())
    }

    async fn list_memberships(
        &self,
        user_id: &str,
        organization_id: &str,
    ) -> Result<Vec<workos_client::types::OrganizationMembership>> {
        let response = workos_ok(
            self.client
                .list_organization_memberships(Some(organization_id), None, Some(user_id))
                .await,
        )
        .await?;
        Ok(response.data)
    }

    async fn create_email_invitation(
        &self,
        organization_id: &str,
        inviter_user_id: &str,
        email: &str,
    ) -> Result<workos_client::types::Invitation> {
        workos_ok(
            self.client
                .create_invitation(&CreateInvitationRequest {
                    email: email.to_owned(),
                    expires_in_days: Some(self.email_invite_days),
                    inviter_user_id: Some(inviter_user_id.to_owned()),
                    organization_id: Some(organization_id.to_owned()),
                    role_slug: Some(self.member_role_slug.clone()),
                })
                .await,
        )
        .await
    }

    async fn refresh_cli_session(&self, refresh_token: &str) -> Result<CliRefreshResponse> {
        let response = refresh_workos_session(
            &self.base_url,
            &self.api_key,
            &self.client_id,
            refresh_token,
        )
        .await?;
        let claims = self.verify_access_token(&response.access_token).await?;
        Ok(CliRefreshResponse {
            access_token: response.access_token,
            refresh_token: response.refresh_token,
            access_expires_at: format_rfc3339(unix_timestamp(claims.exp)?),
            user: map_current_user(response.user),
        })
    }

    async fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims> {
        let header = decode_header(token).context("invalid access token header")?;
        let kid = header.kid.ok_or_else(|| anyhow!("missing WorkOS key id"))?;
        let key = if let Some(key) = self.jwks_key(&kid, false).await? {
            key
        } else {
            self.jwks_key(&kid, true)
                .await?
                .ok_or_else(|| anyhow!("unknown WorkOS signing key"))?
        };

        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_aud = false;
        validation.set_required_spec_claims(&["exp", "iat", "iss", "sub"]);
        validation.set_issuer(&[self.issuer.as_str()]);

        let decoded = decode::<AccessTokenClaims>(
            token,
            &DecodingKey::from_rsa_components(&key.n, &key.e)
                .context("invalid WorkOS JWKS entry")?,
            &validation,
        )
        .context("invalid WorkOS access token")?;

        Ok(decoded.claims)
    }

    async fn jwks_key(&self, kid: &str, force_refresh: bool) -> Result<Option<CachedJwk>> {
        let mut guard = self.jwks_cache.write().await;
        let is_stale = guard
            .as_ref()
            .map(|cache| {
                cache.fetched_at + Duration::minutes(JWKS_CACHE_MINUTES)
                    <= OffsetDateTime::now_utc()
            })
            .unwrap_or(true);

        if force_refresh || is_stale {
            *guard = Some(fetch_jwks(&self.client, &self.client_id).await?);
        }

        Ok(guard
            .as_ref()
            .and_then(|cache| cache.keys.get(kid))
            .cloned())
    }
}

pub async fn auth_config(State(state): State<AppState>) -> Json<AuthConfigResponse> {
    Json(AuthConfigResponse {
        client_id: state.auth.client_id().to_owned(),
    })
}

pub async fn current_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CurrentUserResponse>, (StatusCode, String)> {
    let user = require_user(&state, &headers).await?;
    let profile = state
        .auth
        .get_user(&user.user_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(map_current_user(profile)))
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
    let (user, membership) = ensure_room_member(&state, &headers, &room_id).await?;
    require_owner(&membership)?;

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

    let (user, membership) = ensure_room_member(&state, &headers, &room_id).await?;
    require_owner(&membership)?;

    let room_access = state
        .db
        .get_room_access(&membership.room_id)
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))?;

    let invitation = state
        .auth
        .create_email_invitation(
            &room_access.workos_organization_id,
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
    let user = require_user(&state, &headers).await?;
    let invite = state
        .db
        .get_link_invite(&hash_token(&request.token))
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, "invite not found".to_owned()))?;

    enforce_link_invite_valid(&invite)?;

    let room_access = state
        .db
        .get_room_access(&invite.room_id)
        .await
        .map_err(internal_error)?
        .ok_or((
            StatusCode::NOT_FOUND,
            format!("room not found: {}", invite.room_id),
        ))?;

    state
        .auth
        .ensure_membership(&user.user_id, &room_access.workos_organization_id)
        .await
        .map_err(internal_error)?;

    let room = resolve_room(&state, &invite.room_id).await?;
    Ok(Json(AcceptInviteResponse {
        room: RoomMetadataResponse {
            room_id: room.room_id,
            name: room.name,
            created_at: room.created_at,
            viewer_role: viewer_role(&room_access, &user.user_id),
        },
    }))
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
    let room_access = state
        .db
        .get_room_access(room_id)
        .await
        .map_err(internal_error)?
        .ok_or((StatusCode::NOT_FOUND, format!("room not found: {room_id}")))?;

    let memberships = state
        .auth
        .list_memberships(&user.user_id, &room_access.workos_organization_id)
        .await
        .map_err(internal_error)?;

    if memberships.is_empty() {
        return Err((StatusCode::FORBIDDEN, "room access denied".to_owned()));
    }

    let role = viewer_role(&room_access, &user.user_id);
    Ok((
        user.clone(),
        RoomMembership {
            room_id: room_access.room_id,
            role,
        },
    ))
}

fn build_workos_client(base_url: &str, api_key: &str) -> Result<workos_client::Client> {
    let mut headers = workos_client::reqwest::header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {api_key}")
            .parse()
            .context("invalid WorkOS API key")?,
    );

    Ok(workos_client::Client::new_with_client(
        base_url,
        workos_client::reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .default_headers(headers)
            .build()
            .context("failed to build WorkOS HTTP client")?,
    ))
}

async fn refresh_workos_session(
    base_url: &str,
    api_key: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<AuthenticateResponse> {
    let url = format!("{}/user_management/authenticate", trim_url(base_url));
    let response = workos_client::reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("failed to build WorkOS refresh client")?
        .post(url)
        .bearer_auth(api_key)
        .json(&RefreshSessionRequest {
            client_id,
            grant_type: "refresh_token",
            refresh_token,
        })
        .send()
        .await
        .context("failed to refresh WorkOS session")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let body = body.trim();
        if body.is_empty() {
            bail!("WorkOS returned {status}");
        }
        bail!("WorkOS returned {status}: {body}");
    }

    response
        .json::<AuthenticateResponse>()
        .await
        .context("invalid WorkOS refresh response")
}

async fn fetch_jwks(client: &workos_client::Client, client_id: &str) -> Result<CachedJwks> {
    let response = workos_ok(client.get_jwks(client_id).await).await?;
    Ok(CachedJwks {
        fetched_at: OffsetDateTime::now_utc(),
        keys: jwks_to_map(response)?,
    })
}

fn jwks_to_map(jwks: JwksResponse) -> Result<HashMap<String, CachedJwk>> {
    let mut keys = HashMap::new();
    for key in jwks.keys {
        keys.insert(key.kid, CachedJwk { n: key.n, e: key.e });
    }
    if keys.is_empty() {
        bail!("WorkOS JWKS response did not contain any keys");
    }
    Ok(keys)
}

async fn workos_ok<T>(
    result: Result<workos_client::ResponseValue<T>, workos_client::Error<()>>,
) -> Result<T> {
    match result {
        Ok(response) => Ok(response.into_inner()),
        Err(workos_client::Error::UnexpectedResponse(response)) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let body = body.trim();
            if body.is_empty() {
                Err(anyhow!("WorkOS returned {status}"))
            } else {
                Err(anyhow!("WorkOS returned {status}: {body}"))
            }
        }
        Err(error) => Err(anyhow!("WorkOS request failed: {error}")),
    }
}

fn viewer_role(room_access: &RoomAccessRecord, user_id: &str) -> String {
    if room_access.owner_workos_user_id == user_id {
        "owner".to_owned()
    } else {
        "member".to_owned()
    }
}

fn require_owner(membership: &RoomMembership) -> Result<(), (StatusCode, String)> {
    if membership.role == "owner" {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "only room owners can manage invites".to_owned(),
        ))
    }
}

fn map_current_user(user: User) -> CurrentUserResponse {
    let display_name = display_name(&user);
    CurrentUserResponse {
        user_id: user.id,
        display_name,
        primary_email: user.email,
        avatar_url: user.profile_picture_url,
    }
}

fn display_name(user: &User) -> String {
    let first = user
        .first_name
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    let last = user.last_name.as_deref().map(str::trim).unwrap_or_default();
    match (first.is_empty(), last.is_empty()) {
        (false, false) => format!("{first} {last}"),
        (false, true) => first.to_owned(),
        (true, false) => last.to_owned(),
        (true, true) => user.email.clone(),
    }
}

fn unix_timestamp(timestamp: usize) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(timestamp as i64).context("invalid access token expiry")
}

fn format_rfc3339(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| timestamp.unix_timestamp().to_string())
}

fn parse_rfc3339(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).with_context(|| format!("invalid timestamp: {value}"))
}

fn generate_link_token() -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, LINK_TOKEN_LENGTH)
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn enforce_link_invite_valid(invite: &LinkInviteRecord) -> Result<(), (StatusCode, String)> {
    if invite.revoked_at.is_some() {
        return Err((StatusCode::GONE, "invite has been revoked".to_owned()));
    }

    let expires_at = parse_rfc3339(&invite.expires_at)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    if expires_at <= OffsetDateTime::now_utc() {
        return Err((StatusCode::GONE, "invite has expired".to_owned()));
    }

    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}

fn required_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("{name} is not set"))
}

fn optional_env_i64(name: &str, default_value: i64) -> Result<i64> {
    match env::var(name) {
        Ok(value) => value
            .parse::<i64>()
            .with_context(|| format!("failed to parse {name} as integer")),
        Err(_) => Ok(default_value),
    }
}

fn unauthorized_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::UNAUTHORIZED, error.to_string())
}
