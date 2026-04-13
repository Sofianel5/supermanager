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
    AuthConfigResponse, CliRefreshRequest, CliRefreshResponse, CreateInviteRequest,
    CreateReporterTokenRequest, CreateReporterTokenResponse, CurrentUserResponse, InviteResponse,
    Room,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::RwLock;
use url::Url;
use workos_client::{
    self,
    types::{
        AuthenticateResponse, CreateInvitationRequest, CreateOrganizationMembershipRequest,
        CreateOrganizationRequest, JwksResponse, User,
    },
};

use crate::store::{RoomAccessRecord, normalize_room_id};

use super::{AppState, internal_error, trim_url};

const DEFAULT_WORKOS_BASE_URL: &str = "https://api.workos.com";
const DEFAULT_MEMBER_ROLE: &str = "member";
const DEFAULT_EMAIL_INVITE_DAYS: i64 = 7;
const JWKS_CACHE_MINUTES: i64 = 60;
const REPORTER_TOKEN_LENGTH: usize = 48;
const REPORTER_TOKEN_PREFIX: &str = "sm_ingest_";

#[derive(Clone, Debug)]
pub struct AuthConfig {
    api_key: String,
    base_url: String,
    client_id: String,
    issuer: String,
    member_role_slug: String,
    email_invite_days: i64,
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

        let client = build_workos_client(&base_url, &api_key)?;

        Ok(Self {
            api_key,
            base_url,
            client_id,
            issuer,
            member_role_slug,
            email_invite_days,
            client,
            jwks_cache: Arc::new(RwLock::new(None)),
        })
    }

    fn client_id(&self) -> &str {
        &self.client_id
    }

    fn api_hostname(&self) -> Option<String> {
        if trim_url(&self.base_url) == DEFAULT_WORKOS_BASE_URL {
            return None;
        }
        Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(ToOwned::to_owned))
    }

    fn api_base_url(&self) -> Option<String> {
        let base_url = trim_url(&self.base_url).to_owned();
        if base_url == DEFAULT_WORKOS_BASE_URL {
            None
        } else {
            Some(base_url)
        }
    }

    async fn create_workspace_organization(
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
        api_hostname: state.auth.api_hostname(),
        api_base_url: state.auth.api_base_url(),
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

pub async fn create_reporter_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(request): Json<CreateReporterTokenRequest>,
) -> Result<Json<CreateReporterTokenResponse>, (StatusCode, String)> {
    let (user, membership) = ensure_room_member(&state, &headers, &room_id).await?;
    let token = generate_reporter_token();
    state
        .db
        .create_reporter_token(
            &membership.room_id,
            &user.user_id,
            request.repo_root.as_deref(),
            &hash_token(&token),
        )
        .await
        .map_err(internal_error)?;

    Ok(Json(CreateReporterTokenResponse { token }))
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

    let room_id = membership.room_id;

    Ok(Json(InviteResponse {
        room_id: room_id.clone(),
        target_email: Some(target_email.to_owned()),
        expires_at: invitation.expires_at,
        invite_url: Some(invitation_redirect_url(
            &state.public_app_url,
            &room_id,
            &invitation.token,
            &invitation.accept_invitation_url,
        )),
    }))
}

pub(crate) async fn require_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<AuthenticatedUser, (StatusCode, String)> {
    let token =
        bearer_token(headers).ok_or((StatusCode::UNAUTHORIZED, "sign in required".to_owned()))?;
    if token.starts_with(REPORTER_TOKEN_PREFIX) {
        return Err((StatusCode::UNAUTHORIZED, "sign in required".to_owned()));
    }

    let claims = state
        .auth
        .verify_access_token(token)
        .await
        .map_err(unauthorized_error)?;
    Ok(AuthenticatedUser {
        user_id: claims.sub,
    })
}

pub(crate) async fn authorize_hook_ingest(
    state: &AppState,
    headers: &HeaderMap,
    room_id: &str,
) -> Result<String, (StatusCode, String)> {
    if let Some(token) = bearer_token(headers)
        && token.starts_with(REPORTER_TOKEN_PREFIX)
    {
        let token_hash = hash_token(token);
        let record = state
            .db
            .get_reporter_token(&token_hash)
            .await
            .map_err(internal_error)?
            .ok_or((
                StatusCode::UNAUTHORIZED,
                "invalid reporter token".to_owned(),
            ))?;
        let room_id = normalize_room_id(room_id);
        if record.room_id != room_id {
            return Err((
                StatusCode::FORBIDDEN,
                "reporter token does not match room".to_owned(),
            ));
        }
        state
            .db
            .touch_reporter_token(&token_hash)
            .await
            .map_err(internal_error)?;
        return Ok(room_id);
    }

    let (_, membership) = ensure_room_member(state, headers, room_id).await?;
    Ok(membership.room_id)
}

pub(crate) async fn create_room_for_user(
    state: &AppState,
    name: &str,
    user_id: &str,
) -> Result<Room, (StatusCode, String)> {
    let name = name.trim();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "room name is required".to_owned()));
    }

    let workspace = match state
        .db
        .get_owned_workspace(user_id)
        .await
        .map_err(internal_error)?
    {
        Some(workspace) => workspace,
        None => {
            let organization = state
                .auth
                .create_workspace_organization(name)
                .await
                .map_err(internal_error)?;
            state
                .auth
                .ensure_membership(user_id, &organization.id)
                .await
                .map_err(internal_error)?;
            state
                .db
                .create_workspace(name, &organization.id, user_id)
                .await
                .map_err(internal_error)?
        }
    };

    state
        .db
        .create_room(workspace.workspace_id, name)
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

    if memberships
        .iter()
        .all(|membership| membership.status != "active")
    {
        return Err((StatusCode::FORBIDDEN, "room access denied".to_owned()));
    }

    let role = viewer_role(&room_access, &user.user_id);
    Ok((
        user,
        RoomMembership {
            room_id: room_access.room_id.clone(),
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
            "only workspace owners can manage invites".to_owned(),
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

fn generate_reporter_token() -> String {
    let mut rng = rand::rng();
    format!(
        "{REPORTER_TOKEN_PREFIX}{}",
        Alphanumeric.sample_string(&mut rng, REPORTER_TOKEN_LENGTH)
    )
}

fn hash_token(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn invitation_redirect_url(
    app_url: &str,
    room_id: &str,
    token: &str,
    fallback_url: &str,
) -> String {
    let login_url = format!("{}/login", trim_url(app_url));
    let Ok(mut url) = Url::parse(&login_url) else {
        return fallback_url.to_owned();
    };
    url.query_pairs_mut()
        .append_pair("invitation_token", token)
        .append_pair("next", &format!("/r/{room_id}"));
    url.into()
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
