//! Axum middleware that authenticates each request against the configured
//! [`oidc_rs::AuthConfig`] and injects [`oidc_rs::Identity`] into the
//! request extensions.

use axum::extract::Request;
use axum::extract::State;
use axum::middleware::Next;
use axum::response::Response;
use oidc_rs::{resolve_identity, AuthMode, AuthState, Identity};
use std::sync::Arc;

use crate::error::auth_error_to_response;

/// Axum middleware that authenticates each request.
///
/// Apply via:
///
/// ```rust,ignore
/// use axum::middleware::from_fn_with_state;
/// use std::sync::Arc;
/// use oidc_rs_axum::{auth_middleware, AuthMode, AuthState};
///
/// let state = Arc::new(AuthState { mode: AuthMode::Disabled });
/// let app = axum::Router::new()
///     .route("/whoami", axum::routing::get(handler))
///     .layer(from_fn_with_state(state, auth_middleware));
/// ```
///
/// The held [`AuthState`] is `Arc`-shared so a single instance can be cloned
/// across the router — the underlying [`oidc_rs::Validator`] and
/// [`oidc_rs::BasicExchanger`] are themselves `Arc`-backed, so clones share
/// the same JWKS cache, refresh task, and credential-exchange cache.
///
/// # Arguments
///
/// * `State(state)` — The shared [`AuthState`] containing the configured
///   [`AuthMode`].
/// * `req` — The incoming request.
/// * `next` — The next middleware/handler in the chain.
///
/// # Returns
///
/// A `Response` — either the downstream handler's response, or an error
/// response (401/503) if authentication fails.
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let identity = match &state.mode {
        AuthMode::Disabled => Identity::Disabled,
        AuthMode::Enabled {
            validator,
            exchanger,
        } => {
            let header = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok());
            match resolve_identity(header, validator, exchanger).await {
                Ok(id) => id,
                Err(e) => return auth_error_to_response(&e),
            }
        }
    };
    req.extensions_mut().insert(identity);
    next.run(req).await
}
