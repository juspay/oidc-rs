# oidc-rs

Lightweight OIDC Resource Server primitives for Rust services. Validates inbound JWTs against an OIDC issuer's JWKS, and exchanges `Authorization: Basic` credentials for JWTs via the IdP's `client_credentials` grant — cached and single-flighted so machine clients don't pay a token-endpoint roundtrip per request.

## Crate structure

This workspace contains two crates:

| Crate | Description |
|-------|-------------|
| [`oidc-rs`](crates/oidc-rs) | Framework-agnostic core: config builder, JWT `Validator`, and `BasicExchanger`. |
| [`oidc-rs-actix`](crates/oidc-rs-actix) | Actix-Web adapter: `AuthMiddleware` + `Authenticated` extractor. Depends on `oidc-rs`. |

## Installation

Both crates are pre-1.0 and not yet published to crates.io. Use a git dependency:

```toml
[dependencies]
oidc-rs = { git = "https://github.com/juspay/oidc-rs.git" }
oidc-rs-actix = { git = "https://github.com/juspay/oidc-rs.git" }
```

## Usage

```rust
use actix_web::HttpResponse;
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_actix::{Authenticated, AuthMiddleware, AuthMode, AuthState};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Build config
    let config = AuthConfig::builder()
        .issuer("https://your-idp.example.com")
        .audiences(["my-api"])
        .build()
        .expect("auth config");

    // Construct validator + exchanger from the enabled config
    let AuthConfig::Enabled(c) = config else {
        panic!("auth must be enabled");
    };
    let validator = Validator::new(
        c.issuer.clone(),
        c.audiences.clone(),
        c.jwks_refresh,
    )
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

    actix_web::HttpServer::new(move || {
        actix_web::App::new()
            .wrap(AuthMiddleware { state: state.clone() })
            .route("/whoami", actix_web::web::get().to(whoami))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}

async fn whoami(auth: Authenticated) -> HttpResponse {
    HttpResponse::Ok().json(&auth.0)
}
```

### Disabled mode

For local development or no-auth environments, skip the validator/exchanger and set `AuthMode::Disabled`:

```rust
use oidc_rs_actix::{AuthMiddleware, AuthMode, AuthState};
use std::sync::Arc;

let state = Arc::new(AuthState { mode: AuthMode::Disabled });
```

Every request then resolves to [`Identity::Disabled`] and handlers run normally.

## Features

- **JWKS cache + background refresh** — the `Validator` fetches the issuer's JWKS at construction and re-fetches on a configurable interval, so key rotations are picked up without restart.
- **Single-flight Basic exchange** — concurrent `client_credentials` requests for the same `client_id` share a single in-flight IdP call, preventing token-endpoint thundering herds.
- **Negative caching** — IdP rejections (400/401/403) are cached for 30 s and transient failures (network/5xx) for 5 s, so a broken or hostile client can't thrash the IdP.
- **Disabled mode** — flip auth off entirely without touching handler code; the middleware emits `Identity::Disabled` and the `Authenticated` extractor still works.
- **Framework-agnostic core** — `Validator` and `BasicExchanger` are plain `async` types with no framework coupling. The Actix adapter is a thin layer; adapters for other frameworks can be built on the same core.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
