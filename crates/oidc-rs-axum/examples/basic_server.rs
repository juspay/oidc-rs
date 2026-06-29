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
use axum::{middleware::from_fn_with_state, routing, Router};
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

            let validator = Validator::new(c.issuer.clone(), c.audiences.clone(), c.jwks_refresh)
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
    axum::Json(auth.0)
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
