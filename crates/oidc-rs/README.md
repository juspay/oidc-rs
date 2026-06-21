# oidc-rs

Framework-agnostic OIDC Resource Server core for Rust services. Validates inbound JWTs against an OIDC issuer's JWKS, and exchanges `Authorization: Basic` credentials for JWTs via the IdP's `client_credentials` grant — cached, single-flighted, and negative-cached.

This crate has no framework coupling. For Actix-Web users, see [`oidc-rs-actix`](../oidc-rs-actix/README.md).

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
let claims = validator.validate(raw_jwt).await?; // Result<Claims, AuthError>

// Force a JWKS refetch (e.g. after a key rotation)
validator.refresh_jwks().await?; // Result<(), AuthError>
```

## BasicExchanger

`BasicExchanger` performs OIDC discovery to locate the token endpoint, then exchanges `client_id` / `client_secret` for a JWT via the `client_credentials` grant. Successful exchanges are cached until the JWT's expiry (capped by `hard_ttl`); concurrent exchanges for the same `client_id` are single-flighted; IdP rejections and transient failures are negative-cached:

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

## License

Dual-licensed under MIT or Apache-2.0, at your option.
