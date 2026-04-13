use anyhow::{Context, Result, anyhow, bail};
use axum::http::header;
use reporter_protocol::{CliRefreshResponse, CurrentUserResponse};
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use workos_client::types::{
    AuthenticateResponse, CreateInvitationRequest, CreateOrganizationMembershipRequest,
    CreateOrganizationRequest, User,
};

use super::config::AuthConfig;

#[derive(Debug, Serialize)]
pub(crate) struct RefreshSessionRequest<'a> {
    pub(crate) client_id: &'a str,
    pub(crate) grant_type: &'static str,
    pub(crate) refresh_token: &'a str,
}

impl AuthConfig {
    pub(crate) async fn create_room_organization(
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

    pub(crate) async fn get_user(&self, user_id: &str) -> Result<User> {
        workos_ok(self.client.get_user(user_id).await).await
    }

    pub(crate) async fn ensure_membership(
        &self,
        user_id: &str,
        organization_id: &str,
    ) -> Result<()> {
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

    pub(crate) async fn list_memberships(
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

    pub(crate) async fn create_email_invitation(
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

    pub(crate) async fn refresh_cli_session(
        &self,
        refresh_token: &str,
    ) -> Result<CliRefreshResponse> {
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
}

pub(crate) fn build_workos_client(
    base_url: &str,
    api_key: &str,
) -> Result<workos_client::Client> {
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

pub(crate) async fn refresh_workos_session(
    base_url: &str,
    api_key: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<AuthenticateResponse> {
    let url = format!(
        "{}/user_management/authenticate",
        crate::util::trim_url(base_url)
    );
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

pub(crate) async fn workos_ok<T>(
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

pub(crate) fn map_current_user(user: User) -> CurrentUserResponse {
    let display_name = display_name(&user);
    CurrentUserResponse {
        user_id: user.id,
        display_name,
        primary_email: user.email,
        avatar_url: user.profile_picture_url,
    }
}

pub(crate) fn display_name(user: &User) -> String {
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

pub(crate) fn unix_timestamp(timestamp: usize) -> Result<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(timestamp as i64).context("invalid access token expiry")
}

fn format_rfc3339(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&Rfc3339)
        .unwrap_or_else(|_| timestamp.unix_timestamp().to_string())
}

pub(crate) fn parse_rfc3339(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339).with_context(|| format!("invalid timestamp: {value}"))
}
