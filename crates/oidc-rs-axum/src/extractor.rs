//! Extractor that reads the [`Identity`] injected by [`auth_middleware`].

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use oidc_rs::Identity;

/// Pulls the [`Identity`] out of the request extensions. If the middleware
/// is not configured, returns HTTP 500 — that's a programming error.
pub struct Authenticated(pub Identity);

impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Identity>()
            .cloned()
            .map(Authenticated)
            .ok_or_else(|| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({
                        "error": {
                            "code": "INTERNAL_ERROR",
                            "message": "auth middleware not configured"
                        }
                    })),
                )
                    .into_response()
            })
    }
}
