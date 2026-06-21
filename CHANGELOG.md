# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `oidc-rs` crate: framework-agnostic OIDC Resource Server core with `Validator`, `BasicExchanger`, and `AuthConfig` builder.
- `oidc-rs-actix` crate: Actix-Web adapter with `AuthMiddleware` and `Authenticated` extractor.
- `basic_server` example with Keycloak Docker quickstart guide.
- CI workflow running `fmt-check`, `clippy`, and `test` on every PR.
- Dual MIT / Apache-2.0 licensing.
