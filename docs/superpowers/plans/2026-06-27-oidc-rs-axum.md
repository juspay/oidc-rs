# oidc-rs-axum Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `oidc-rs-axum` crate (Axum 0.8 adapter) and extract shared auth-resolution logic from the actix adapter into the `oidc-rs` core crate.

**Architecture:** Three crates are touched: (1) `oidc-rs` core gets a new `resolve.rs` module with `resolve_identity` and a `state.rs` module with `AuthState`/`AuthMode`; (2) `oidc-rs-actix` is refactored to delegate to core; (3) `oidc-rs-axum` is a new crate mirroring the actix adapter's structure (middleware fn, `Authenticated` extractor, error mapping) using `axum::middleware::from_fn_with_state`.

**Tech Stack:** Rust 2021, edition 2021, rust-version 1.75, Axum 0.8, tower 0.5, tokio 1, base64 0.22

**Spec:** `docs/superpowers/specs/2026-06-27-oidc-rs-axum-design.md`

---

## File Map

### Core crate (`crates/oidc-rs/`)
- **Create:** `src/state.rs` — `AuthState` + `AuthMode` types (moved from actix)
- **Create:** `src/resolve.rs` — `resolve_identity` fn + `strip_scheme_prefix` helper (moved from actix) + unit tests
- **Modify:** `src/lib.rs` — add `mod state; mod resolve;` + re-exports
- **Modify:** `Cargo.toml` — add `base64 = "0.22"` dependency

### Actix crate (`crates/oidc-rs-actix/`)
- **Modify:** `src/middleware.rs` — remove local `resolve_identity`, `strip_scheme_prefix`, `AuthState`, `AuthMode`; delegate to `oidc_rs::resolve_identity`
- **Modify:** `src/lib.rs` — re-export `AuthMode`/`AuthState` from `oidc_rs` instead of local `middleware`
- **Modify:** `Cargo.toml` — remove `base64 = "0.22"` dependency

### New axum crate (`crates/oidc-rs-axum/`)
- **Create:** `Cargo.toml`
- **Create:** `src/lib.rs`
- **Create:** `src/error.rs`
- **Create:** `src/extractor.rs`
- **Create:** `src/middleware.rs`
- **Create:** `tests/smoke.rs`
- **Create:** `examples/basic_server.rs`
- **Create:** `examples/README.md`
- **Create:** `README.md`

### Workspace / docs
- **Modify:** `Cargo.toml` (root) — add workspace member
- **Modify:** `README.md` (root) — add crate row + Axum usage example
- **Modify:** `justfile` — add axum example recipes
- **Modify:** `CHANGELOG.md` — add entries

---

## Task 1: Core crate — add `state.rs` with `AuthState`/`AuthMode`

**Files:**
- Create: `crates/oidc-rs/src/state.rs`
- Modify: `crates/oidc-rs/src/lib.rs`

- [ ] **Step 1: Create `crates/oidc-rs/src/state.rs`**

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

- [ ] **Step 2: Update `crates/oidc-rs/src/lib.rs` — add module + re-exports**

Replace the entire contents of `crates/oidc-rs/src/lib.rs` with:

```rust
//! Lightweight OIDC Resource Server primitives for Rust services.
//!
//! See the crate README for an overview.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

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

Note: `mod resolve;` is declared here but the file doesn't exist yet — the build will fail until Task 2 Step 1. That's expected; we complete both modules before building.

- [ ] **Step 3: Proceed to Task 2 (do not build yet)**

The `mod resolve;` declaration references a file that doesn't exist yet. We create it in the next task before attempting to build.

---

## Task 2: Core crate — add `resolve.rs` with `resolve_identity` + tests

**Files:**
- Modify: `crates/oidc-rs/Cargo.toml` — add `base64` dep
- Create: `crates/oidc-rs/src/resolve.rs`

- [ ] **Step 1: Add `base64` dependency to `crates/oidc-rs/Cargo.toml`**

Add `base64 = "0.22"` to the `[dependencies]` section. The full `[dependencies]` block should read:

```toml
[dependencies]
openidconnect = "4"
jsonwebtoken = { version = "10", features = ["rust_crypto"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["sync", "time", "rt"] }
tracing = "0.1"
dashmap = "5"
sha2 = "0.10"
base64 = "0.22"
```

- [ ] **Step 2: Create `crates/oidc-rs/src/resolve.rs` with the resolve function + helper + unit tests**

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_scheme_prefix_matches_case_insensitively() {
        assert_eq!(
            strip_scheme_prefix("Bearer abc", "Bearer"),
            Some("abc")
        );
        assert_eq!(
            strip_scheme_prefix("bearer abc", "Bearer"),
            Some("abc")
        );
        assert_eq!(
            strip_scheme_prefix("BEARER abc", "Bearer"),
            Some("abc")
        );
        assert_eq!(
            strip_scheme_prefix("Basic dXNlcjpwYXNz", "Basic"),
            Some("dXNlcjpwYXNz")
        );
    }

    #[test]
    fn strip_scheme_prefix_rejects_no_space_after_scheme() {
        assert_eq!(strip_scheme_prefix("BearerToken abc", "Bearer"), None);
    }

    #[test]
    fn strip_scheme_prefix_rejects_too_short() {
        assert_eq!(strip_scheme_prefix("", "Bearer"), None);
        assert_eq!(strip_scheme_prefix("Bearer", "Bearer"), None);
    }

    #[test]
    fn strip_scheme_prefix_trims_leading_whitespace_after_scheme() {
        assert_eq!(
            strip_scheme_prefix("Bearer    abc", "Bearer"),
            Some("abc")
        );
    }
}
```

- [ ] **Step 3: Build the core crate to verify it compiles**

Run: `cargo build -p oidc-rs`
Expected: compiles with no errors

- [ ] **Step 4: Run core crate tests to verify unit tests pass**

Run: `cargo test -p oidc-rs`
Expected: all tests pass (existing tests + new `strip_scheme_prefix` tests)

- [ ] **Step 5: Commit**

```bash
git add crates/oidc-rs/src/state.rs crates/oidc-rs/src/resolve.rs crates/oidc-rs/src/lib.rs crates/oidc-rs/Cargo.toml
git commit -m "refactor(oidc-rs): extract resolve_identity and AuthState/AuthMode into core"
```

---

## Task 3: Actix crate — refactor to delegate to core

**Files:**
- Modify: `crates/oidc-rs-actix/src/middleware.rs`
- Modify: `crates/oidc-rs-actix/src/lib.rs`
- Modify: `crates/oidc-rs-actix/Cargo.toml`

- [ ] **Step 1: Replace `crates/oidc-rs-actix/src/middleware.rs`**

Replace the entire file with this refactored version that delegates to `oidc_rs::resolve_identity` and uses `oidc_rs::{AuthMode, AuthState}` instead of local definitions:

```rust
//! Actix `Transform` that authenticates each request against the configured
//! [`oidc_rs::AuthConfig`] and injects [`oidc_rs::Identity`] into the request
//! extensions.

use actix_web::body::BoxBody;
use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage};
use oidc_rs::{resolve_identity, AuthMode, AuthState, Identity};
use std::future::{self, Future, Ready};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use crate::error::to_response;

/// Auth middleware "factory". Configure once at startup and wrap any scope.
///
/// The held [`AuthState`] is `Arc`-shared so a single instance can be cloned
/// across `HttpServer` workers — the underlying [`oidc_rs::Validator`] and
/// [`oidc_rs::BasicExchanger`] are themselves `Arc`-backed, so per-worker
/// clones share the same JWKS cache, refresh task, and credential-exchange
/// cache.
#[derive(Clone)]
pub struct AuthMiddleware {
    /// Shared auth state.
    pub state: Arc<AuthState>,
}

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        future::ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
            state: self.state.clone(),
        }))
    }
}

#[doc(hidden)]
pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
    state: Arc<AuthState>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: actix_web::body::MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let state = self.state.clone();
        Box::pin(async move {
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
        })
    }
}
```

Key changes from the original:
- Removed: `AuthState`, `AuthMode`, `AuthState::exchanger` (now in `oidc_rs::state`)
- Removed: `resolve_identity`, `strip_scheme_prefix` (now in `oidc_rs::resolve`)
- Removed: `use base64::Engine as _;` and `use oidc_rs::{AuthError, BasicExchanger, Validator};` (no longer needed)
- Added: `use oidc_rs::{resolve_identity, AuthMode, AuthState, Identity};`
- The `Service::call` body now calls `oidc_rs::resolve_identity(header, ...)` instead of the local `resolve_identity`

- [ ] **Step 2: Replace `crates/oidc-rs-actix/src/lib.rs`**

```rust
//! Actix-Web adapter for [`oidc-rs`](https://docs.rs/oidc-rs).
//!
//! Provides an [`AuthMiddleware`] that authenticates inbound requests against
//! the configured [`oidc_rs::AuthConfig`] (Bearer JWT validation + Basic→JWT
//! exchange) and injects an [`oidc_rs::Identity`] into request extensions.
//! The [`Authenticated`] extractor reads that identity in handlers.

#![deny(rust_2018_idioms)]
#![warn(missing_docs)]

mod error;
mod extractor;
mod middleware;

pub use error::to_response as auth_error_to_response;
pub use extractor::Authenticated;
pub use middleware::AuthMiddleware;
pub use oidc_rs::{AuthMode, AuthState};
```

Key change: `AuthMode` and `AuthState` are now re-exported from `oidc_rs` instead of from the local `middleware` module.

- [ ] **Step 3: Remove `base64` from `crates/oidc-rs-actix/Cargo.toml`**

Remove the `base64 = "0.22"` line from `[dependencies]`. The block should now read:

```toml
[dependencies]
oidc-rs = { path = "../oidc-rs" }
actix-web = "4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
```

- [ ] **Step 4: Build the actix crate to verify it compiles**

Run: `cargo build -p oidc-rs-actix`
Expected: compiles with no errors

- [ ] **Step 5: Run actix crate tests to verify refactor didn't break anything**

Run: `cargo test -p oidc-rs-actix`
Expected: the existing smoke test passes (disabled-mode test exercises the middleware → extensions → extractor wiring)

- [ ] **Step 6: Run clippy on the actix crate**

Run: `cargo clippy -p oidc-rs-actix --all-targets -- -D warnings`
Expected: no warnings

- [ ] **Step 7: Commit**

```bash
git add crates/oidc-rs-actix/src/middleware.rs crates/oidc-rs-actix/src/lib.rs crates/oidc-rs-actix/Cargo.toml
git commit -m "refactor(oidc-rs-actix): delegate to core's resolve_identity"
```

---

## Task 4: Axum crate — scaffold + `error.rs`

**Files:**
- Modify: `Cargo.toml` (root) — add workspace member
- Create: `crates/oidc-rs-axum/Cargo.toml`
- Create: `crates/oidc-rs-axum/src/lib.rs`
- Create: `crates/oidc-rs-axum/src/error.rs`

- [ ] **Step 1: Add the new crate to the workspace in root `Cargo.toml`**

Change the `members` list in `[workspace]` to:

```toml
[workspace]
members = ["crates/oidc-rs", "crates/oidc-rs-actix", "crates/oidc-rs-axum"]
resolver = "2"
```

- [ ] **Step 2: Create `crates/oidc-rs-axum/Cargo.toml`**

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

- [ ] **Step 3: Create `crates/oidc-rs-axum/src/lib.rs`**

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

Note: `mod extractor;` and `mod middleware;` are declared but the files don't exist yet. The build will fail until Tasks 5 and 6 create them. That's expected.

- [ ] **Step 4: Create `crates/oidc-rs-axum/src/error.rs`**

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
```

- [ ] **Step 5: Proceed to Task 5 (do not build yet)**

The `mod extractor;` and `mod middleware;` declarations reference files that don't exist yet. We create them in the next tasks before attempting to build.

---

## Task 5: Axum crate — `extractor.rs`

**Files:**
- Create: `crates/oidc-rs-axum/src/extractor.rs`

- [ ] **Step 1: Create `crates/oidc-rs-axum/src/extractor.rs`**

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

- [ ] **Step 2: Proceed to Task 6 (do not build yet)**

The `mod middleware;` declaration still references a file that doesn't exist. We create it in the next task.

---

## Task 6: Axum crate — `middleware.rs` + smoke test

**Files:**
- Create: `crates/oidc-rs-axum/src/middleware.rs`
- Create: `crates/oidc-rs-axum/tests/smoke.rs`

- [ ] **Step 1: Create `crates/oidc-rs-axum/src/middleware.rs`**

```rust
//! Axum middleware that authenticates each request against the configured
//! [`oidc_rs::AuthConfig`] and injects [`oidc_rs::Identity`] into the
//! request extensions.

use axum::extract::State;
use axum::extract::Request;
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
```

- [ ] **Step 2: Build the axum crate to verify it compiles**

Run: `cargo build -p oidc-rs-axum`
Expected: compiles with no errors

- [ ] **Step 3: Create `crates/oidc-rs-axum/tests/smoke.rs`**

```rust
//! Smoke test: the middleware in disabled mode injects Identity::Disabled.

use axum::{middleware, routing, Router};
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState, Authenticated};
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn disabled_mode_injects_disabled_identity() {
    async fn whoami(auth: Authenticated) -> axum::Json<oidc_rs::Identity> {
        axum::Json(auth.0)
    }

    let state = Arc::new(AuthState {
        mode: AuthMode::Disabled,
    });
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({"type": "disabled"}));
}
```

- [ ] **Step 4: Run the smoke test to verify it passes**

Run: `cargo test -p oidc-rs-axum`
Expected: `disabled_mode_injects_disabled_identity` passes

- [ ] **Step 5: Run clippy on the axum crate**

Run: `cargo clippy -p oidc-rs-axum --all-targets -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add crates/oidc-rs-axum/Cargo.toml crates/oidc-rs-axum/src/lib.rs crates/oidc-rs-axum/src/error.rs crates/oidc-rs-axum/src/extractor.rs crates/oidc-rs-axum/src/middleware.rs crates/oidc-rs-axum/tests/smoke.rs Cargo.toml
git commit -m "feat(oidc-rs-axum): Axum adapter crate with auth_middleware and Authenticated extractor"
```

---

## Task 7: Axum crate — example `basic_server.rs` + `examples/README.md`

**Files:**
- Create: `crates/oidc-rs-axum/examples/basic_server.rs`
- Create: `crates/oidc-rs-axum/examples/README.md`

- [ ] **Step 1: Create `crates/oidc-rs-axum/examples/basic_server.rs`**

```rust
//! Minimal Axum server showing how to wire `oidc-rs-axum`.
//!
//! Run with:
//!
//! ```sh
//! # disabled mode (no IdP required)
//! cargo run -p oidc-rs-axum --example basic_server
//!
//! # enabled mode (requires a reachable OIDC issuer)
//! OIDC_ISSUER=https://your-idp.example.com \
//! OIDC_AUDIENCES=my-api \
//! cargo run -p oidc-rs-axum --example basic_server
//! ```
//!
//! Then test with:
//!
//! ```sh
//! # disabled mode — no auth header needed
//! curl http://localhost:8080/whoami
//!
//! # enabled mode — Bearer JWT
//! curl -H "Authorization: Bearer <jwt>" http://localhost:8080/whoami
//!
//! # enabled mode — Basic credentials (client_id:client_secret)
//! curl -u client-id:client-secret http://localhost:8080/whoami
//! ```

use std::sync::Arc;

use axum::response::IntoResponse;
use axum::{routing, Router, middleware::from_fn_with_state};
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState, Authenticated};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    let state = build_auth_state().await;
    let bind = ("127.0.0.1", 8080);

    println!("Listening on http://{}:{}", bind.0, bind.1);
    if matches!(state.mode, AuthMode::Disabled) {
        println!("Auth: DISABLED — all requests pass through");
    } else {
        println!("Auth: ENABLED — requests require a Bearer JWT or Basic credentials");
    }

    let app = Router::new()
        .route("/whoami", routing::get(whoami))
        .route("/protected", routing::get(protected))
        .layer(from_fn_with_state(state, auth_middleware));

    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the [`AuthState`] from env vars, falling back to disabled mode
/// when `OIDC_ISSUER` is unset.
async fn build_auth_state() -> Arc<AuthState> {
    let issuer = std::env::var("OIDC_ISSUER").ok();
    let audiences: Vec<String> = std::env::var("OIDC_AUDIENCES")
        .ok()
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    match (issuer, audiences.is_empty()) {
        (Some(issuer), false) => {
            let config = AuthConfig::builder()
                .issuer(&issuer)
                .audiences(&audiences)
                .build()
                .expect("auth config");

            let AuthConfig::Enabled(c) = &config else {
                unreachable!()
            };

            let validator =
                Validator::new(c.issuer.clone(), c.audiences.clone(), c.jwks_refresh)
                    .await
                    .expect("validator");

            let exchanger = BasicExchanger::new(
                c.issuer.clone(),
                c.basic_audience.clone(),
                c.basic_scope.clone(),
                c.basic_cache_ttl,
            )
            .await
            .expect("exchanger");

            Arc::new(AuthState {
                mode: AuthMode::Enabled {
                    validator,
                    exchanger,
                },
            })
        }
        _ => {
            eprintln!("OIDC_ISSUER or OIDC_AUDIENCES not set — starting in DISABLED mode");
            Arc::new(AuthState {
                mode: AuthMode::Disabled,
            })
        }
    }
}

/// Returns the authenticated identity as JSON.
async fn whoami(auth: Authenticated) -> impl IntoResponse {
    axum::Json(&auth.0)
}

/// A protected endpoint that reads the identity's subject.
async fn protected(auth: Authenticated) -> impl IntoResponse {
    match &auth.0 {
        oidc_rs::Identity::Disabled => "Auth is disabled — this endpoint is open".to_string(),
        oidc_rs::Identity::Bearer(claims) | oidc_rs::Identity::Basic(claims) => {
            format!("Hello, {}!", claims.sub)
        }
    }
}
```

- [ ] **Step 2: Create `crates/oidc-rs-axum/examples/README.md`**

```markdown
# basic_server example

A minimal Axum server showing how to wire `oidc-rs-axum` middleware, extractors, and error handling.

## Run in disabled mode

No IdP required — all requests pass through with `Identity::Disabled`.

```sh
cargo run -p oidc-rs-axum --example basic_server
```

```sh
curl http://localhost:8080/whoami
# {"type":"disabled"}
```

## Run in enabled mode with Keycloak

### 1. Start Keycloak

```sh
docker run -d \
  --name keycloak \
  -p 8484:8080 \
  -e KEYCLOAK_ADMIN=admin \
  -e KEYCLOAK_ADMIN_PASSWORD=admin \
  quay.io/keycloak/keycloak:26.0 \
  start-dev \
  --http-port=8080
```

Wait for Keycloak to be ready (usually 10–15 seconds):

```sh
until curl -sf http://localhost:8484/realms/master/.well-known/openid-configuration >/dev/null; do
  echo "waiting for Keycloak..."; sleep 2
done
```

### 2. Configure a client and audience

Using `kcadm` (shipped inside the Keycloak container):

```sh
# Authenticate kcadm
docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  config credentials \
  --server http://localhost:8080 \
  --realm master \
  --user admin \
  --password admin

# Create a client for the example API
docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  create clients \
  -r master \
  -s clientId=my-api \
  -s "redirectUris=[\"http://localhost:8080/callback\"]" \
  -s publicClient=false \
  -s secret=my-api-secret \
  -s serviceAccountsEnabled=true \
  -s directAccessGrantsEnabled=true

# Create a client for a machine caller
docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  create clients \
  -r master \
  -s clientId=m2m-client \
  -s publicClient=false \
  -s secret=m2m-secret \
  -s serviceAccountsEnabled=true \
  -s directAccessGrantsEnabled=true

# Add an audience mapper so m2m-client tokens include my-api in aud
M2M_UUID=$(docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  get clients -r master --fields id,clientId \
  | jq -r '.[] | select(.clientId=="m2m-client") | .id')

docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  create clients/$M2M_UUID/protocol-mappers/models \
  -r master \
  -s name=audience-my-api \
  -s protocol=openid-connect \
  -s protocolMapper=oidc-audience-mapper \
  -s 'config."included.client.audience"="my-api"' \
  -s 'config."access.token.claim"="true"'
```

### 3. Start the example server

```sh
OIDC_ISSUER=http://localhost:8484/realms/master \
OIDC_AUDIENCES=my-api \
cargo run -p oidc-rs-axum --example basic_server
```

### 4. Test it

**Bearer (machine-to-machine via client_credentials):**

```sh
# Exchange client credentials for a JWT
JWT=$(curl -sf http://localhost:8484/realms/master/protocol/openid-connect/token \
  -d grant_type=client_credentials \
  -d client_id=m2m-client \
  -d client_secret=m2m-secret \
  | jq -r .access_token)

# Call the protected endpoint
curl -sf -H "Authorization: Bearer $JWT" http://localhost:8080/whoami
```

**Basic (the library exchanges for you):**

```sh
curl -sf -u m2m-client:m2m-secret http://localhost:8080/whoami
```

**Missing/invalid credentials:**

```sh
curl -i http://localhost:8080/whoami
# HTTP/1.1 401 Unauthorized
# {"error":{"code":"MISSING_AUTHORIZATION","message":"missing Authorization header"}}
```

## Endpoints

| Method | Path        | Description |
|--------|-------------|-------------|
| GET    | `/whoami`   | Returns the authenticated `Identity` as JSON |
| GET    | `/protected` | Returns a greeting using `claims.sub` |

## Cleanup

```sh
docker rm -f keycloak
```
```

- [ ] **Step 3: Build the example to verify it compiles**

Run: `cargo build -p oidc-rs-axum --example basic_server`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/oidc-rs-axum/examples/basic_server.rs crates/oidc-rs-axum/examples/README.md
git commit -m "feat(oidc-rs-axum): add basic_server example with Keycloak quickstart guide"
```

---

## Task 8: Axum crate — `README.md`

**Files:**
- Create: `crates/oidc-rs-axum/README.md`

- [ ] **Step 1: Create `crates/oidc-rs-axum/README.md`**

```markdown
# oidc-rs-axum

Axum adapter for [`oidc-rs`](../oidc-rs/README.md). Provides an `auth_middleware` function that authenticates every request (Bearer JWT validation + Basic→JWT exchange) and an `Authenticated` extractor for handlers.

## Status

Pre-1.0; the API may evolve.

## Installation

Not yet published to crates.io. Use a git dependency:

```toml
[dependencies]
oidc-rs-axum = { git = "https://github.com/juspay/oidc-rs.git" }
```

## Usage

Apply `auth_middleware` to your `Router` via `from_fn_with_state`, then use the `Authenticated` extractor in handlers:

```rust
use axum::response::IntoResponse;
use axum::{routing, Router, middleware::from_fn_with_state};
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState, Authenticated};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let config = AuthConfig::builder()
        .issuer("https://idp.example.com")
        .audiences(["my-api"])
        .build()
        .expect("auth config");

    let AuthConfig::Enabled(c) = config else {
        panic!("auth must be enabled");
    };
    let validator = Validator::new(c.issuer.clone(), c.audiences.clone(), c.jwks_refresh)
        .await
        .unwrap();
    let exchanger = BasicExchanger::new(
        c.issuer.clone(),
        c.basic_audience.clone(),
        c.basic_scope.clone(),
        c.basic_cache_ttl,
    )
    .await
    .unwrap();

    let state = Arc::new(AuthState {
        mode: AuthMode::Enabled { validator, exchanger },
    });

    let app = Router::new()
        .route("/whoami", routing::get(whoami))
        .layer(from_fn_with_state(state, auth_middleware));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn whoami(auth: Authenticated) -> impl IntoResponse {
    axum::Json(&auth.0)
}
```

### Disabled mode

For local development, set `AuthMode::Disabled` — the middleware injects `Identity::Disabled` and every request passes through:

```rust
use oidc_rs_axum::{auth_middleware, AuthMode, AuthState};
use std::sync::Arc;

let state = Arc::new(AuthState { mode: AuthMode::Disabled });
let app = axum::Router::new()
    .route("/whoami", axum::routing::get(handler))
    .layer(axum::middleware::from_fn_with_state(state, auth_middleware));
```

## Error mapping

`AuthError` is mapped to a `Response` by `auth_error_to_response`:

- **401 Unauthorized** for client-side credential failures (missing/malformed header, expired token, bad signature/issuer/audience, rejected credentials).
- **503 Service Unavailable** (`Retry-After: 5`) for IdP failures — both transport-level (`IdpUnreachable`) and protocol-level (`IdpMalformedResponse`). The IdP returning garbage isn't the client's fault.

The public response body uses stable, code-specific messages and never leaks issuer URLs, IdP response fragments, or JWT internals. The full internal error is logged at `warn` before responding.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
```

- [ ] **Step 2: Commit**

```bash
git add crates/oidc-rs-axum/README.md
git commit -m "docs(oidc-rs-axum): add crate README"
```

---

## Task 9: Workspace docs — root `README.md`, `justfile`, `CHANGELOG.md`

**Files:**
- Modify: `README.md` (root)
- Modify: `justfile`
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Update root `README.md` — add axum crate to the table**

In the "Crate structure" section, add a third row to the table. Find the table that currently reads:

```markdown
| Crate | Description |
|-------|-------------|
| [`oidc-rs`](crates/oidc-rs) | Framework-agnostic core: config builder, JWT `Validator`, and `BasicExchanger`. |
| [`oidc-rs-actix`](crates/oidc-rs-actix) | Actix-Web adapter: `AuthMiddleware` + `Authenticated` extractor. Depends on `oidc-rs`. |
```

Replace it with:

```markdown
| Crate | Description |
|-------|-------------|
| [`oidc-rs`](crates/oidc-rs) | Framework-agnostic core: config builder, JWT `Validator`, and `BasicExchanger`. |
| [`oidc-rs-actix`](crates/oidc-rs-actix) | Actix-Web adapter: `AuthMiddleware` + `Authenticated` extractor. Depends on `oidc-rs`. |
| [`oidc-rs-axum`](crates/oidc-rs-axum) | Axum adapter: `auth_middleware` + `Authenticated` extractor. Depends on `oidc-rs`. |
```

- [ ] **Step 2: Update root `README.md` — update the description paragraph**

Change the first sentence of the "Crate structure" section from:

```markdown
This workspace contains two crates:
```

to:

```markdown
This workspace contains three crates:
```

- [ ] **Step 3: Update root `README.md` — add Axum to the Installation section**

In the Installation section, add `oidc-rs-axum` to the git dependency example. The block should read:

```toml
[dependencies]
oidc-rs = { git = "https://github.com/juspay/oidc-rs.git" }
oidc-rs-actix = { git = "https://github.com/juspay/oidc-rs.git" }
oidc-rs-axum = { git = "https://github.com/juspay/oidc-rs.git" }
```

- [ ] **Step 4: Update root `README.md` — add "Framework-agnostic core" mention**

In the Features section, update the last bullet to mention the Axum adapter:

```markdown
- **Framework-agnostic core** — `Validator` and `BasicExchanger` are plain `async` types with no framework coupling. The Actix and Axum adapters are thin layers; adapters for other frameworks can be built on the same core.
```

- [ ] **Step 5: Add axum example recipes to `justfile`**

Add these recipes after the existing `example-disabled` recipe (before the `example-bearer` recipe):

```makefile

# --- Axum example server (crates/oidc-rs-axum/examples/basic_server.rs) ---

# run the axum example server in enabled mode against local Keycloak
example-axum:
    OIDC_ISSUER=http://localhost:{{KC_PORT}}/realms/{{KC_REALM}} \
    OIDC_AUDIENCES=my-api \
    cargo run -p oidc-rs-axum --example basic_server

# run the axum example server in disabled mode (no IdP required)
example-axum-disabled:
    cargo run -p oidc-rs-axum --example basic_server
```

- [ ] **Step 6: Update `CHANGELOG.md` — add entries under `[Unreleased]`**

In the `### Added` section, add these two lines after the existing entries:

```markdown
- `oidc-rs` crate: `resolve_identity` function and `AuthState`/`AuthMode` types (extracted from actix adapter).
- `oidc-rs-axum` crate: Axum adapter with `auth_middleware` and `Authenticated` extractor.
```

- [ ] **Step 7: Commit**

```bash
git add README.md justfile CHANGELOG.md
git commit -m "docs: update README, justfile, and CHANGELOG for oidc-rs-axum"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run formatting check**

Run: `cargo fmt --check`
Expected: no formatting issues

If there are issues, run `cargo fmt` to fix them, then re-run the check.

- [ ] **Step 2: Run clippy across the entire workspace**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no warnings across all three crates

- [ ] **Step 3: Run all tests across the entire workspace**

Run: `cargo test`
Expected: all tests pass across all three crates:
- `oidc-rs`: existing tests + new `strip_scheme_prefix` tests
- `oidc-rs-actix`: existing `smoke.rs` test
- `oidc-rs-axum`: new `disabled_mode_injects_disabled_identity` test

- [ ] **Step 4: Build all examples**

Run: `cargo build --examples`
Expected: both examples compile (actix `basic_server` + axum `basic_server`)

- [ ] **Step 5: If any fixes were needed in steps 1-4, commit them**

```bash
git add -A
git commit -m "fix: address fmt/clippy/test issues from final verification"
```

If no fixes were needed, skip this step.
