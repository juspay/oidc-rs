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
