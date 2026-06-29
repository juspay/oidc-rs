//! Axum adapter for [`oidc-rs`](https://docs.rs/oidc-rs).
//!
//! Provides an [`auth_middleware`] function that authenticates inbound
//! requests against the [`oidc_rs::AuthState`] (Bearer JWT
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
