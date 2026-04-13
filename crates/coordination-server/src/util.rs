use axum::http::StatusCode;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn format_rfc3339(value: OffsetDateTime) -> String {
    value.format(&Rfc3339).unwrap()
}

pub fn now_rfc3339() -> String {
    format_rfc3339(OffsetDateTime::now_utc())
}

pub fn internal_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

pub fn service_unavailable_error(error: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
}

pub fn trim_url(url: &str) -> &str {
    url.trim_end_matches('/')
}
