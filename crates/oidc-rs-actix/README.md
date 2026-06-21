# oidc-rs-actix

Actix-Web adapter for [`oidc-rs`](../oidc-rs/README.md). Provides an `AuthMiddleware` that authenticates every request (Bearer JWT validation + Basic→JWT exchange) and an `Authenticated` extractor for handlers.

## Status

Pre-1.0; the API may evolve.

## Installation

Not yet published to crates.io. Use a git dependency:

```toml
[dependencies]
oidc-rs-actix = { git = "https://github.com/juspay/oidc-rs.git" }
```

## Usage

Wrap your `App` (or a scope) with `AuthMiddleware`, then use the `Authenticated` extractor in handlers:

```rust
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_actix::{Authenticated, AuthMiddleware, AuthMode, AuthState};
use std::sync::Arc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config = AuthConfig::builder()
        .issuer("https://idp.example.com")
        .audiences(["my-api"])
        .build()
        .unwrap();

    let (validator, exchanger) = match config {
        oidc_rs::AuthConfig::Enabled(c) => {
            let v = Validator::new(c.issuer.clone(), c.audiences.clone(), c.jwks_refresh)
                .await
                .unwrap();
            let e = BasicExchanger::new(
                c.issuer.clone(),
                c.basic_audience.clone(),
                c.basic_scope.clone(),
                c.basic_cache_ttl,
            )
            .await
            .unwrap();
            (v, e)
        }
        _ => unreachable!(),
    };

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

async fn whoami(auth: Authenticated) -> impl actix_web::Responder {
    serde_json::to_value(&auth.0)
}
```

### Disabled mode

For local development, set `AuthMode::Disabled` — the middleware injects `Identity::Disabled` and every request passes through:

```rust
use oidc_rs_actix::{AuthMiddleware, AuthMode, AuthState};
use std::sync::Arc;

let state = Arc::new(AuthState { mode: AuthMode::Disabled });
```

## Error mapping

`AuthError` is mapped to an `HttpResponse` by `auth_error_to_response`:

- **401 Unauthorized** for client-side credential failures (missing/malformed header, expired token, bad signature/issuer/audience, rejected credentials).
- **503 Service Unavailable** (`Retry-After: 5`) for IdP failures — both transport-level (`IdpUnreachable`) and protocol-level (`IdpMalformedResponse`). The IdP returning garbage isn't the client's fault.

The public response body uses stable, code-specific messages and never leaks issuer URLs, IdP response fragments, or JWT internals. The full internal error is logged at `warn` before responding.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
