//! Map [`oidc_rs::AuthError`] to an Axum [`Response`].

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use oidc_rs::AuthError;

/// Build an HTTP error response for the given auth failure.
///
/// * 401 for client-side credential failures (missing/malformed/expired/etc).
/// * 503 (with `Retry-After: 5`) for IdP failures — both transport-level
///   (`IdpUnreachable`) and protocol-level (`IdpMalformedResponse`). The IdP
///   returning garbage isn't the client's fault, so we shouldn't tell them
///   to fix their credentials.
///
/// The response body's `message` field uses a stable, code-specific string
/// (see [`public_message`]) so we never leak issuer URLs, IdP response
/// fragments, JWT internals, etc. into the public surface. The full internal
/// error is logged at `warn` before returning.
///
/// # Arguments
///
/// * `err` — The [`AuthError`] to map to an HTTP response.
///
/// # Returns
///
/// A `Response` with status 401 (client errors) or 503 (IdP errors),
/// with a JSON body containing `error.code` and `error.message`.
pub fn auth_error_to_response(err: &AuthError) -> Response {
    tracing::warn!("auth failed: {err}");

    let body = serde_json::json!({
        "error": { "code": code(err), "message": public_message(err) }
    });
    match err {
        AuthError::IdpUnreachable(_) | AuthError::IdpMalformedResponse(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [("Retry-After", "5")],
            axum::Json(body),
        )
            .into_response(),
        _ => (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response(),
    }
}

fn code(err: &AuthError) -> &'static str {
    match err {
        AuthError::MissingHeader => "MISSING_AUTHORIZATION",
        AuthError::MalformedHeader => "MALFORMED_AUTHORIZATION",
        AuthError::IdpRejected => "INVALID_CREDENTIALS",
        AuthError::Expired => "TOKEN_EXPIRED",
        AuthError::BadSignature => "BAD_SIGNATURE",
        AuthError::BadIssuer(_) => "BAD_ISSUER",
        AuthError::BadAudience => "BAD_AUDIENCE",
        AuthError::IdpUnreachable(_) => "IDP_UNREACHABLE",
        AuthError::IdpMalformedResponse(_) => "IDP_MALFORMED_RESPONSE",
    }
}

/// Stable, code-specific public message. Never includes runtime detail
/// (issuer URLs, IdP response fragments, JWT-decode errors, etc.) — those
/// belong in server logs, not in the response a misbehaving client could
/// scrape.
fn public_message(err: &AuthError) -> &'static str {
    match err {
        AuthError::MissingHeader => "missing Authorization header",
        AuthError::MalformedHeader => "malformed Authorization header",
        AuthError::Expired => "token expired",
        AuthError::BadSignature => "token signature invalid",
        AuthError::BadIssuer(_) => "token issuer not accepted",
        AuthError::BadAudience => "token audience not accepted",
        AuthError::IdpRejected => "invalid credentials",
        AuthError::IdpUnreachable(_) => "identity provider unreachable",
        AuthError::IdpMalformedResponse(_) => "identity provider returned an unexpected response",
    }
}
