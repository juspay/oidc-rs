# oidc-rs

Framework-agnostic OIDC Resource Server core for Rust services. Validates inbound JWTs against an OIDC issuer's JWKS, and exchanges `Authorization: Basic` credentials for JWTs via the IdP's `client_credentials` grant — cached, single-flighted, and negative-cached.

This crate has no framework coupling. Framework adapters:
- [Actix-Web](../oidc-rs-actix/README.md) — `AuthMiddleware` + `Authenticated` extractor
- [Axum](../oidc-rs-axum/README.md) — `auth_middleware` + `Authenticated` extractor

## Status

Pre-1.0; the API may evolve.

## Installation

Not yet published to crates.io. Use a git dependency:

```toml
[dependencies]
oidc-rs = { git = "https://github.com/juspay/oidc-rs.git" }
```

## Configuration

Use [`AuthConfig::builder()`] to construct an enabled config. `issuer` and `audiences` are required; the rest are optional with sensible defaults:

```rust
use oidc_rs::AuthConfig;
use std::time::Duration;

let config = AuthConfig::builder()
    .issuer("https://idp.example.com")
    .audiences(["my-api", "my-dashboard"])
    .basic_audience("my-api")              // optional: audience for Basic→JWT exchange
    .basic_scope("jobs.read jobs.write")   // optional: scope for Basic→JWT exchange
    .basic_cache_ttl(Duration::from_secs(600)) // optional, default 1h
    .jwks_refresh(Duration::from_secs(120))    // optional, default 5min
    .build()?; // Result<AuthConfig, BuildError>
```

For local-dev / no-auth modes, use [`AuthConfig::disabled()`] instead.

## Validator

`Validator` discovers the issuer (`<issuer>/.well-known/openid-configuration`), fetches the JWKS, and spawns a background refresh task:

```rust
use oidc_rs::{Validator, AuthError};

let validator = Validator::new(
    "https://idp.example.com".to_string(),
    vec!["my-api".to_string()],
    std::time::Duration::from_secs(300),
)
.await?; // Result<Validator, AuthError>

// Validate a raw JWT — returns Claims on success
let raw_jwt = "<jwt from Authorization header>";
let claims = validator.validate(raw_jwt).await?; // Result<Claims, AuthError>

// Force a JWKS refetch (e.g. after a key rotation)
validator.refresh_jwks().await?; // Result<(), AuthError>
```

## BasicExchanger

`BasicExchanger` performs OIDC discovery to locate the token endpoint, then exchanges `client_id` / `client_secret` for a JWT via the `client_credentials` grant. Successful exchanges are cached until the earlier of `expires_in - 60s` and `hard_ttl - 60s`; concurrent exchanges for the same `client_id` are single-flighted; IdP rejections and transient failures are negative-cached:

```rust
use oidc_rs::BasicExchanger;
use std::time::Duration;

let exchanger = BasicExchanger::new(
    "https://idp.example.com".to_string(),
    Some("my-api".to_string()),   // audience — None to omit
    Some("jobs.read".to_string()), // scope — None to omit
    Duration::from_secs(3600),     // hard cap on positive cache lifetime
)
.await?; // Result<BasicExchanger, AuthError>

// Exchange Basic credentials for a JWT (cached, single-flighted)
let jwt = exchanger.exchange("client-id", "secret").await?; // Result<String, AuthError>

// Flush cache entries — pass None to flush all
let (positive_evicted, negative_evicted) = exchanger.flush(None);
```

## AuthState and AuthMode

`AuthState` and `AuthMode` are the shared types that framework adapters build on. `AuthMode` is a discriminant — `Disabled` (bypass all auth) or `Enabled { validator, exchanger }` — and `AuthState` is a thin wrapper holding the mode:

```rust
use oidc_rs::{AuthMode, AuthState, Validator, BasicExchanger};
use std::sync::Arc;

// Enabled — adapters call resolve_identity with these
let state = Arc::new(AuthState {
    mode: AuthMode::Enabled { validator, exchanger },
});

// Disabled — adapters short-circuit to Identity::Disabled
let state = Arc::new(AuthState { mode: AuthMode::Disabled });
```

Adapters match on `state.mode`: in `Disabled` they inject `Identity::Disabled` directly; in `Enabled` they extract the `Authorization` header and call [`resolve_identity`](#resolve_identity).

## resolve_identity

`resolve_identity` is the framework-agnostic function that all adapters delegate to. Given a raw `Authorization` header value (or `None`), it validates a Bearer JWT or exchanges Basic credentials, returning an `Identity`:

```rust
use oidc_rs::{resolve_identity, AuthError, Identity};

// header: Option<&str> — the raw Authorization header value
let identity: Result<Identity, AuthError> =
    resolve_identity(header, &validator, &exchanger).await;
```

On success you get `Identity::Bearer(claims)`, `Identity::Basic(claims)`, or (in disabled mode, handled by the adapter) `Identity::Disabled`. On failure you get an `AuthError` that the adapter maps to an HTTP response (401 for client errors, 503 + `Retry-After` for IdP errors).

## Building a new adapter

To add support for a new framework, you need three pieces:

1. **Middleware** — extract the `Authorization` header from the request, match on `AuthMode`, and either inject `Identity::Disabled` (disabled mode) or call `resolve_identity(header, &validator, &exchanger).await` (enabled mode). On `Ok`, insert the `Identity` into request extensions; on `Err`, map the `AuthError` to an HTTP response.

2. **Extractor** — a newtype wrapping `Identity` that implements the framework's request-extraction trait, reading `Identity` from request extensions.

3. **Error mapping** — convert `AuthError` to the framework's response type: 401 for client-side failures (missing/malformed header, expired token, bad signature/issuer/audience, rejected credentials), 503 with `Retry-After: 5` for IdP failures (`IdpUnreachable`, `IdpMalformedResponse`). Use stable, code-specific public messages and never leak issuer URLs, IdP response fragments, or JWT internals — log the full error at `warn` before returning.

See the [`oidc-rs-actix`](../oidc-rs-actix) and [`oidc-rs-axum`](../oidc-rs-axum) crates for reference implementations.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
