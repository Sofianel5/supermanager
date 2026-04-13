use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};
use workos_client::types::JwksResponse;

use super::config::AuthConfig;

#[derive(Debug, Clone)]
pub(crate) struct CachedJwks {
    pub(crate) fetched_at: OffsetDateTime,
    pub(crate) keys: HashMap<String, CachedJwk>,
}

#[derive(Debug, Clone)]
pub(crate) struct CachedJwk {
    pub(crate) n: String,
    pub(crate) e: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AccessTokenClaims {
    pub(crate) sub: String,
    pub(crate) exp: usize,
}

const JWKS_CACHE_MINUTES: i64 = 60;

impl AuthConfig {
    pub(crate) async fn verify_access_token(&self, token: &str) -> Result<AccessTokenClaims> {
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

    pub(crate) async fn jwks_key(
        &self,
        kid: &str,
        force_refresh: bool,
    ) -> Result<Option<CachedJwk>> {
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

pub(crate) async fn fetch_jwks(
    client: &workos_client::Client,
    client_id: &str,
) -> Result<CachedJwks> {
    let response = super::workos::workos_ok(client.get_jwks(client_id).await).await?;
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
