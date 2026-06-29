# Design: `oidc-rs-axum` Crate

**Date:** 2026-06-27
**Status:** Approved (pending spec review)

## Summary

Add a third workspace crate, `oidc-rs-axum`, that provides an Axum 0.8 adapter
for the `oidc-rs` OIDC Resource Server library. This mirrors the existing
`oidc-rs-actix` adapter: an auth middleware, an `Authenticated` extractor, and
an error-to-response mapping.

To avoid duplicating the framework-agnostic auth-resolution logic (header
parsing, Bearer/Basic dispatch, base64 decoding) across two adapter crates, that
logic is extracted from the actix crate into the `oidc-rs` core crate as a
public `resolve_identity` function. The `AuthState` and `AuthMode` configuration
types are also moved to the core crate so both adapters share them.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Where shared auth logic lives | Extract into `oidc-rs` core | DRY single source of truth; core stays framework-agnostic |
| Axum version | 0.8 (latest stable) | Current major; modern `from_fn_with_state` API |
| Middleware style | `axum::middleware::from_fn_with_state` | Idiomatic Axum 0.8; minimal boilerplate; recommended by Axum docs |
| `AuthState`/`AuthMode` location | Move to `oidc-rs` core | Framework-agnostic; both adapters share identical definitions |
| Backwards compatibility | Not a constraint | Pre-1.0, no adopters yet — free to refactor actix crate |

## Architecture

Three crates are touched — one new, two modified:

```
crates/
├── oidc-rs/          ← MODIFIED: extract shared auth-resolution logic + state types
├── oidc-rs-actix/    ← MODIFIED: refactor to delegate to core; drop base64 dep
└── oidc-rs-axum/     ← NEW: Axum 0.8 adapter
```

### What moves where

| Logic | Before | After |
|-------|--------|-------|
| `resolve_identity` (Bearer/Basic → `Identity`) | actix `middleware.rs` (private fn) | `oidc-rs` core `resolve.rs` (public fn) |
| `strip_scheme_prefix` (scheme matcher) | actix `middleware.rs` (private fn) | `oidc-rs` core `resolve.rs` (private helper) |
| base64 decoding for Basic credentials | actix crate (`base64` dep) | `oidc-rs` core (`base64` dep) |
| `AuthState` / `AuthMode` | actix `middleware.rs` | `oidc-rs` core `state.rs` |

## Core Crate Changes (`oidc-rs`)

### New module: `src/resolve.rs`

Contains the framework-agnostic auth-resolution logic extracted from the actix
adapter:

```rust
//! Framework-agnostic Authorization-header resolution: given a raw header
//! value, validate a Bearer JWT or exchange Basic credentials, returning
//! an [`Identity`].

use crate::{AuthError, BasicExchanger, Identity, Validator};
use base64::Engine as _;

/// Resolve an [`Identity`] from a raw `Authorization` header value.
///
/// - `None` (no header) returns [`AuthError::MissingHeader`].
/// - `Bearer <jwt>` — validates the JWT directly via `validator`.
/// - `Basic <base64(client_id:secret)>` — exchanges credentials via
///   `exchanger`, then validates the resulting JWT via `validator`.
/// - Unrecognised scheme or malformed value returns
///   [`AuthError::MalformedHeader`].
///
/// # Arguments
///
/// * `auth_header` — The raw `Authorization` header value (or `None` if
///   absent).
/// * `validator` — The JWT validator for signature/claims verification.
/// * `exchanger` — The Basic→JWT exchanger for `client_credentials` grant.
///
/// # Returns
///
/// `Ok(Identity::Bearer(claims))` or `Ok(Identity::Basic(claims))` on
/// success; `Err(AuthError)` on failure.
pub async fn resolve_identity(
    auth_header: Option<&str>,
    validator: &Validator,
    exchanger: &BasicExchanger,
) -> Result<Identity, AuthError> {
    let header = auth_header.ok_or(AuthError::MissingHeader)?;
    if let Some(token) = strip_scheme_prefix(header, "Bearer") {
        let claims = validator.validate(token).await?;
        Ok(Identity::Bearer(claims))
    } else if let Some(b64) = strip_scheme_prefix(header, "Basic") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| AuthError::MalformedHeader)?;
        let s = std::str::from_utf8(&decoded).map_err(|_| AuthError::MalformedHeader)?;
        let (id, secret) = s.split_once(':').ok_or(AuthError::MalformedHeader)?;
        let jwt = exchanger.exchange(id, secret).await?;
        let claims = validator.validate(&jwt).await?;
        Ok(Identity::Basic(claims))
    } else {
        Err(AuthError::MalformedHeader)
    }
}

/// Strip a case-insensitive auth-scheme prefix (RFC 7235 § 2.1). The scheme
/// token must be followed by a literal SP separator; surrounding whitespace
/// on the credential is left for the caller to trim.
///
/// Returns `None` when the header doesn't begin with `<scheme> ` regardless
/// of case, so e.g. `"bearer abc"`, `"BEARER abc"`, and `"Bearer abc"` all
/// match `strip_scheme_prefix(header, "Bearer")`, while `"BearerToken abc"`
/// (no space) and `"" / "Bearer"` (too short) do not.
fn strip_scheme_prefix<'a>(header: &'a str, scheme: &str) -> Option<&'a str> {
    let prefix_len = scheme.len();
    if header.len() <= prefix_len {
        return None;
    }
    if !header[..prefix_len].eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = &header[prefix_len..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some(rest.trim_start())
}
```

### New module: `src/state.rs`

`AuthState` and `AuthMode` moved from the actix crate, unchanged in shape:

```rust
//! Shared auth-state types used by framework adapter middleware.

use crate::{BasicExchanger, Validator};

/// Shared, runtime-built state. Holds the configured mode.
#[derive(Clone)]
pub struct AuthState {
    /// Auth mode: disabled (synthetic [`crate::Identity::Disabled`]) or
    /// enabled with a [`Validator`] + [`BasicExchanger`].
    pub mode: AuthMode,
}

/// Auth mode discriminator.
#[derive(Clone)]
pub enum AuthMode {
    /// Bypass all auth.
    Disabled,
    /// Validate inbound credentials.
    Enabled {
        /// JWT validator.
        validator: Validator,
        /// Basic→JWT exchanger.
        exchanger: BasicExchanger,
    },
}

impl AuthState {
    /// Borrow the underlying exchanger, if any. Useful for cache-flush
    /// endpoints.
    ///
    /// # Returns
    ///
    /// `Some(&BasicExchanger)` when in enabled mode, `None` when disabled.
    pub fn exchanger(&self) -> Option<&BasicExchanger> {
        match &self.mode {
            AuthMode::Enabled { exchanger, .. } => Some(exchanger),
            AuthMode::Disabled => None,
        }
    }
}
```

### `src/lib.rs` — updated re-exports

```rust
mod config;
mod exchanger;
mod identity;
mod resolve;
mod state;
mod validator;

pub use config::{AuthConfig, AuthConfigBuilder, BuildError, EnabledConfig};
pub use exchanger::BasicExchanger;
pub use identity::{AuthError, Claims, Identity};
pub use resolve::resolve_identity;
pub use state::{AuthMode, AuthState};
pub use validator::Validator;
```

### `Cargo.toml` — add `base64`

```toml
[dependencies]
# ... existing deps ...
base64 = "0.22"
```

## Actix Crate Refactor (`oidc-rs-actix`)

### `src/middleware.rs` — slim down

Remove the local `resolve_identity`, `strip_scheme_prefix`, `AuthState`, and
`AuthMode` definitions. The `Service::call` body delegates to
`oidc_rs::resolve_identity`:

```rust
use oidc_rs::{resolve_identity, AuthError, AuthMode, Identity};

// In Service::call:
async move {
    let identity = match &state.mode {
        AuthMode::Disabled => Identity::Disabled,
        AuthMode::Enabled { validator, exchanger } => {
            let header = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok());
            match resolve_identity(header, validator, exchanger).await {
                Ok(id) => id,
                Err(e) => {
                    let response = to_response(&e);
                    return Ok(req
                        .error_response(actix_web::error::InternalError::from_response(
                            "auth", response,
                        ))
                        .map_into_boxed_body());
                }
            }
        }
    };
    req.extensions_mut().insert(identity);
    service.call(req).await.map(|r| r.map_into_boxed_body())
}
```

### `src/lib.rs` — re-export from core

```rust
mod error;
mod extractor;
mod middleware;

pub use error::to_response as auth_error_to_response;
pub use extractor::Authenticated;
pub use middleware::AuthMiddleware;
pub use oidc_rs::{AuthMode, AuthState};
```

### `Cargo.toml` — drop `base64`

```toml
[dependencies]
# ... remove: base64 = "0.22"
```

No changes to `error.rs`, `extractor.rs`, `tests/smoke.rs`, or the example —
they reference `AuthState`/`AuthMode`/`Authenticated` which remain available
via re-exports.

## New Crate: `oidc-rs-axum`

### `Cargo.toml`

```toml
[package]
name = "oidc-rs-axum"
description = "Axum adapter for the oidc-rs OIDC Resource Server library."
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license = "MIT OR Apache-2.0"
readme = "README.md"
keywords = ["oidc", "axum", "jwt", "auth", "openid"]
categories = ["authentication", "web-programming::http-server"]

[dependencies]
oidc-rs = { path = "../oidc-rs" }
axum = "0.8"
serde_json = "1"
tracing = "0.1"

[dev-dependencies]
tracing-subscriber = "0.3"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
```

### `src/lib.rs`

```rust
//! Axum adapter for [`oidc-rs`](https://docs.rs/oidc-rs).
//!
//! Provides an [`auth_middleware`] function that authenticates inbound
//! requests against the configured [`oidc_rs::AuthConfig`] (Bearer JWT
//! validation + Basic→JWT exchange) and injects an [`oidc_rs::Identity`]
//! into request extensions. The [`Authenticated`] extractor reads that
//! identity in handlers.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

mod error;
mod extractor;
mod middleware;

pub use error::auth_error_to_response;
pub use extractor::Authenticated;
pub use middleware::auth_middleware;
pub use oidc_rs::{AuthMode, AuthState};
```

### `src/middleware.rs`

```rust
//! Axum middleware that authenticates each request against the configured
//! [`oidc_rs::AuthConfig`] and injects [`oidc_rs::Identity`] into the
//! request extensions.

use axum::extract::State;
use axum::http::Request;
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
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let identity = match &state.mode {
        AuthMode::Disabled => Identity::Disabled,
        AuthMode::Enabled { validator, exchanger } => {
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
```

Returns `Response` directly (not `Result`) to avoid needing
`impl IntoResponse for AuthError`, which the orphan rule forbids (both
`IntoResponse` and `AuthError` are foreign to the axum crate). This mirrors
the actix middleware, which returns `Ok(req.error_response(...))` on error
rather than `Err(...)`.

### `src/extractor.rs`

```rust
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
```

The newtype wrapper is **required by the orphan rule** — we cannot implement
`FromRequestParts` for the foreign `Identity` type directly. Mirrors the actix
`Authenticated(pub Identity)`.

### `src/error.rs`

```rust
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
pub fn auth_error_to_response(err: &AuthError) -> Response {
    tracing::warn!("auth failed: {err}");

    let body = serde_json::json!({
        "error": { "code": code(err), "message": public_message(err) }
    });
    match err {
        AuthError::IdpUnreachable(_) | AuthError::IdpMalformedResponse(_) => {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Retry-After", "5")],
                axum::Json(body),
            )
                .into_response()
        }
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
```

Identical mapping logic to the actix `error.rs` (401 for client errors, 503 +
`Retry-After: 5` for IdP errors), using Axum's tuple `IntoResponse` instead of
Actix's response builder.

## Example

### `examples/basic_server.rs`

Mirrors the actix `basic_server.rs` structure: reads `OIDC_ISSUER` /
`OIDC_AUDIENCES` env vars, falls back to disabled mode, and exposes `/whoami`
+ `/protected` endpoints. Uses Axum's router setup:

```rust
use std::sync::Arc;
use axum::{routing, Router, middleware::from_fn_with_state};
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState, Authenticated};
use axum::response::IntoResponse;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let state = build_auth_state().await;

    println!("Listening on http://127.0.0.1:8080");
    if matches!(state.mode, AuthMode::Disabled) {
        println!("Auth: DISABLED — all requests pass through");
    } else {
        println!("Auth: ENABLED — requests require a Bearer JWT or Basic credentials");
    }

    let app = Router::new()
        .route("/whoami", routing::get(whoami))
        .route("/protected", routing::get(protected))
        .layer(from_fn_with_state(state, auth_middleware));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn build_auth_state() -> Arc<AuthState> {
    // Same env-var logic as the actix example:
    // OIDC_ISSUER + OIDC_AUDIENCES → Enabled; else Disabled.
    // ...
}

async fn whoami(auth: Authenticated) -> impl IntoResponse {
    axum::Json(&auth.0)
}

async fn protected(auth: Authenticated) -> impl IntoResponse {
    match &auth.0 {
        oidc_rs::Identity::Disabled => "Auth is disabled — this endpoint is open".to_string(),
        oidc_rs::Identity::Bearer(c) | oidc_rs::Identity::Basic(c) => {
            format!("Hello, {}!", c.sub)
        }
    }
}
```

### `examples/README.md`

Same Keycloak Docker quickstart guide as the actix example, with Axum
commands substituted.

## Tests

### `tests/smoke.rs`

Disabled-mode integration test mirroring the actix smoke test. Builds a
`Router` with `from_fn_with_state` in disabled mode, sends a GET to
`/whoami`, asserts `{"type":"disabled"}`:

```rust
use axum::{routing, Router, middleware};
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState, Authenticated};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn disabled_mode_injects_disabled_identity() {
    async fn whoami(auth: Authenticated) -> axum::Json<oidc_rs::Identity> {
        axum::Json(auth.0)
    }

    let state = Arc::new(AuthState { mode: AuthMode::Disabled });
    let app = Router::new()
        .route("/whoami", routing::get(whoami))
        .layer(middleware::from_fn_with_state(state, auth_middleware));

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/whoami")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({"type": "disabled"}));
}
```

Uses `tower::ServiceExt::oneshot` (available via the `axum` dep, which
re-exports `tower`). No `.with_state()` is needed — `from_fn_with_state`
captures the state in the middleware layer; the Router stays `Router<()>`.

## Workspace & Documentation Changes

### Root `Cargo.toml`

Add the new member:

```toml
[workspace]
members = ["crates/oidc-rs", "crates/oidc-rs-actix", "crates/oidc-rs-axum"]
```

### Root `README.md`

Add a third row to the crate table:

| Crate | Description |
|-------|-------------|
| `oidc-rs` | Framework-agnostic core: config builder, JWT `Validator`, and `BasicExchanger`. |
| `oidc-rs-actix` | Actix-Web adapter: `AuthMiddleware` + `Authenticated` extractor. Depends on `oidc-rs`. |
| `oidc-rs-axum` | Axum adapter: `auth_middleware` + `Authenticated` extractor. Depends on `oidc-rs`. |

Add an Axum usage example alongside the existing Actix one.

### Crate-level `README.md` (`crates/oidc-rs-axum/README.md`)

Mirrors the actix crate README: status, installation (git dep), usage example
(enabled + disabled modes), error mapping section, license.

### `justfile`

Add example recipes mirroring the actix ones:

```makefile
# run the axum example server in enabled mode against local Keycloak
example-axum:
    OIDC_ISSUER=http://localhost:{{KC_PORT}}/realms/{{KC_REALM}} \
    OIDC_AUDIENCES=my-api \
    cargo run -p oidc-rs-axum --example basic_server

# run the axum example server in disabled mode (no IdP required)
example-axum-disabled:
    cargo run -p oidc-rs-axum --example basic_server
```

The existing `build`, `test`, `clippy`, `fmt-check`, and `check` recipes
already run across all workspace members, so no changes needed there.

### CI (`.github/workflows/ci.yml`)

No changes. It runs `just fmt-check && just clippy && just test` which covers
all workspace members automatically.

### `CHANGELOG.md`

Add entries under `[Unreleased]`:

```
### Added

- `oidc-rs` crate: `resolve_identity` function and `AuthState`/`AuthMode` types (extracted from actix adapter).
- `oidc-rs-axum` crate: Axum adapter with `auth_middleware` and `Authenticated` extractor.
```

## Error Handling

The error mapping is identical to the actix adapter:

- **401 Unauthorized** for client-side credential failures: missing header,
  malformed header, expired token, bad signature, bad issuer, bad audience,
  rejected credentials.
- **503 Service Unavailable** (`Retry-After: 5`) for IdP failures: both
  transport-level (`IdpUnreachable`) and protocol-level
  (`IdpMalformedResponse`). The IdP returning garbage isn't the client's
  fault.

The public response body uses stable, code-specific messages and never leaks
issuer URLs, IdP response fragments, or JWT internals. The full internal error
is logged at `warn` before responding.

## Testing Strategy

1. **Core crate**: the existing `validator.rs` and `exchanger.rs` unit tests
   continue to pass unchanged. The extracted `resolve_identity` function is
   tested indirectly via the adapter integration tests (which exercise the
   full Bearer + Basic paths against a mock IdP). A dedicated unit test for
   `strip_scheme_prefix` case-insensitivity can be added to `resolve.rs`.

2. **Actix crate**: the existing `tests/smoke.rs` continues to pass unchanged
   (disabled-mode test doesn't exercise `resolve_identity`).

3. **Axum crate**: `tests/smoke.rs` mirrors the actix smoke test (disabled
   mode). This validates the middleware → extensions → extractor wiring
   without needing a live IdP.

## Out of Scope

- Middleware that enforces scope/role requirements (e.g., `requires_scope("admin")`).
  The current adapters authenticate but don't authorize; this is unchanged.
- Integration tests against a live IdP for the axum crate (the actix crate
  also doesn't have these; the `demo` justfile recipe covers that via Keycloak).
- `impl IntoResponse for AuthError` in the axum crate (orphan rule forbids it;
  the `auth_error_to_response` function serves the same purpose).
- A combined `oidc-rs-axum` + `oidc-rs-actix` feature-gated crate. Separate
  adapter crates per framework is the established pattern.
