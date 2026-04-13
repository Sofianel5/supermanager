use std::{env, sync::Arc};

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use super::jwt::CachedJwks;
use super::workos::build_workos_client;

const DEFAULT_WORKOS_BASE_URL: &str = "https://api.workos.com";
const DEFAULT_MEMBER_ROLE: &str = "member";
const DEFAULT_EMAIL_INVITE_DAYS: i64 = 7;
const DEFAULT_LINK_INVITE_DAYS: i64 = 14;

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: String,
    pub(crate) client_id: String,
    pub(crate) issuer: String,
    pub(crate) member_role_slug: String,
    pub(crate) email_invite_days: i64,
    pub(crate) link_invite_days: i64,
    pub(crate) client: workos_client::Client,
    pub(crate) jwks_cache: Arc<RwLock<Option<CachedJwks>>>,
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

    pub fn client_id(&self) -> &str {
        &self.client_id
    }
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
