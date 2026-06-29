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
