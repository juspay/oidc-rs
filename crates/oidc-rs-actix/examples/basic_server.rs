//! Minimal Actix-Web server showing how to wire `oidc-rs-actix`.
//!
//! Run with:
//!
//! ```sh
//! # disabled mode (no IdP required)
//! cargo run -p oidc-rs-actix --example basic_server
//!
//! # enabled mode (requires a reachable OIDC issuer)
//! TE_OIDC_ISSUER=https://your-idp.example.com \
//! TE_OIDC_AUDIENCES=my-api \
//! cargo run -p oidc-rs-actix --example basic_server
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

use actix_web::{web, App, HttpResponse, HttpServer};
use oidc_rs::{AuthConfig, BasicExchanger, Validator};
use oidc_rs_actix::{AuthMiddleware, AuthMode, AuthState, Authenticated};

#[actix_web::main]
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

    HttpServer::new(move || {
        App::new()
            .wrap(AuthMiddleware {
                state: state.clone(),
            })
            .route("/whoami", web::get().to(whoami))
            .route("/protected", web::get().to(protected))
    })
    .bind(bind)?
    .run()
    .await
}

/// Build the [`AuthState`] from env vars, falling back to disabled mode
/// when `TE_OIDC_ISSUER` is unset.
async fn build_auth_state() -> Arc<AuthState> {
    let issuer = std::env::var("TE_OIDC_ISSUER").ok();
    let audiences: Vec<String> = std::env::var("TE_OIDC_AUDIENCES")
        .ok()
        .map(|s| s.split(',').map(String::from).collect())
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
            eprintln!("TE_OIDC_ISSUER or TE_OIDC_AUDIENCES not set — starting in DISABLED mode");
            Arc::new(AuthState {
                mode: AuthMode::Disabled,
            })
        }
    }
}

/// Returns the authenticated identity as JSON.
async fn whoami(auth: Authenticated) -> HttpResponse {
    HttpResponse::Ok().json(&auth.0)
}

/// A protected endpoint that reads the identity's subject.
async fn protected(auth: Authenticated) -> HttpResponse {
    match &auth.0 {
        oidc_rs::Identity::Disabled => {
            HttpResponse::Ok().body("Auth is disabled — this endpoint is open")
        }
        oidc_rs::Identity::Bearer(claims) | oidc_rs::Identity::Basic(claims) => {
            HttpResponse::Ok().body(format!("Hello, {}!", claims.sub))
        }
    }
}
