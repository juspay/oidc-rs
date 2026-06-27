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
    axum::Json(auth.0)
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
