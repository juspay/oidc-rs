# Standalone oidc-rs + oidc-rs-actix Repo — Design

**Status:** Approved
**Date:** 2026-06-21
**Author:** brainstorming session with Natarajan

---

## Summary

Extract the `oidc-rs` (framework-agnostic OIDC Resource Server primitives) and `oidc-rs-actix` (Actix-Web adapter) crates from the [Kronos PR #45](https://github.com/juspay/kronos/pull/45) into a standalone repository at `/Users/natarajankannan/src/oidc-rs`. The crates provide JWT validation against an OIDC issuer's JWKS and Basic-credential-to-JWT exchange for Rust HTTP services. Consumed via git dependency — no crates.io publishing.

---

## Repo structure

```
oidc-rs/                         ← repo root
├── .gitignore
├── Cargo.toml                   ← workspace (members + inlined dep versions)
├── justfile                     ← task runner
├── README.md                    ← top-level overview + quickstart
├── crates/
│   ├── oidc-rs/                 ← framework-agnostic core
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── config.rs
│   │   │   ├── identity.rs
│   │   │   ├── validator.rs
│   │   │   └── exchanger.rs
│   │   └── tests/
│   │       └── fixtures/
│   │           ├── extract_jwk.sh
│   │           ├── test_rsa_priv.pem
│   │           ├── test_rsa_n.txt
│   │           └── test_rsa_e.txt
│   └── oidc-rs-actix/           ← Actix-Web adapter
│       ├── Cargo.toml            (path dep → ../oidc-rs)
│       ├── README.md
│       ├── src/
│       │   ├── lib.rs
│       │   ├── middleware.rs
│       │   ├── extractor.rs
│       │   └── error.rs
│       └── tests/
│           └── smoke.rs
```

---

## Workspace Cargo.toml

A Cargo workspace with two members. The `oidc-rs-actix` crate depends on `oidc-rs` via `path = "../oidc-rs"`, which works for git-based consumption.

```toml
[workspace]
members = ["crates/oidc-rs", "crates/oidc-rs-actix"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
```

---

## Dependency declarations

The PR's Cargo.toml files use `workspace = true` for all deps (inherited from the kronos workspace). We inline the actual versions into each crate's Cargo.toml so the crates are self-contained.

### `crates/oidc-rs/Cargo.toml`

```toml
[package]
name = "oidc-rs"
description = "Lightweight OIDC Resource Server: JWT validation + Basic→JWT exchange for Rust services."
version.workspace = true
edition.workspace = true
license = "MIT OR Apache-2.0"
readme = "README.md"
keywords = ["oidc", "oauth2", "jwt", "auth", "openid"]
categories = ["authentication", "web-programming"]

[dependencies]
openidconnect = "4"
jsonwebtoken = { version = "10", features = ["rust_crypto"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["sync", "time", "rt"] }
tracing = "0.1"
dashmap = "5"
sha2 = "0.10"

[dev-dependencies]
wiremock = "0.6"
```

### `crates/oidc-rs-actix/Cargo.toml`

```toml
[package]
name = "oidc-rs-actix"
description = "Actix-Web adapter for the oidc-rs OIDC Resource Server library."
version.workspace = true
edition.workspace = true
license = "MIT OR Apache-2.0"
readme = "README.md"
keywords = ["oidc", "actix", "actix-web", "jwt", "auth"]
categories = ["authentication", "web-programming::http-server"]

[dependencies]
oidc-rs = { path = "../oidc-rs" }
actix-web = "4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
tracing = "0.1"
```

### tokio feature slimming

The PR uses `tokio = { features = ["full"] }`. For a library crate this forces all transitive consumers to compile all of tokio. Slimmed to `["sync", "time", "rt"]` — the only features the code actually uses:

- `sync` — `tokio::sync::RwLock`, `tokio::sync::Mutex`
- `time` — `tokio::time::interval`
- `rt` — `tokio::spawn` (background JWKS refresh)

---

## Code fidelity

**No source code changes.** The crates are already framework-agnostic by design:

- `oidc-rs` has zero HTTP-framework dependencies. It exposes `AuthConfig` (builder), `Validator` (JWKS + JWT validation), `BasicExchanger` (cached credential exchange), `Identity`/`Claims`/`AuthError` types.
- `oidc-rs-actix` wraps the core with `AuthMiddleware` (Actix `Transform`), `Authenticated` extractor, and `AuthError → HttpResponse` mapping.

The code carries `#![deny(rust_2018_idioms)]` and `#![warn(missing_docs)]` lints, is well-documented, and includes unit tests in each module plus an integration smoke test. All source files are copied verbatim from the PR branch.

---

## Test fixtures

The validator tests reference three fixture files via `include_bytes!` / `include_str!`:

- `tests/fixtures/test_rsa_priv.pem` — RSA private key for minting test JWTs
- `tests/fixtures/test_rsa_n.txt` — base64url-encoded modulus
- `tests/fixtures/test_rsa_e.txt` — base64url-encoded exponent (`AQAB`)
- `tests/fixtures/extract_jwk.sh` — script that regenerates n/e from the PEM

All four are copied as-is.

---

## Justfile

A task runner for common development commands:

```just
# default recipe — list available commands
default:
    @just --list

# build all crates
build:
    cargo build

# run all tests
test:
    cargo test

# run clippy lints
clippy:
    cargo clippy -- -D warnings

# check formatting
fmt-check:
    cargo fmt --check

# apply formatting
fmt:
    cargo fmt

# run all checks (fmt + clippy + test)
check: fmt-check clippy test
```

---

## READMEs

The PR READMEs reference Kronos and use relative paths. Rewritten as standalone:

### Top-level `README.md`

- What the crates do (OIDC RS: JWT validation + Basic→JWT exchange)
- Two-crate structure (framework-agnostic core + Actix adapter)
- Git dependency usage example
- Minimal usage example: configure `AuthConfig`, build `AuthState`, wrap an Actix scope, use `Authenticated` extractor
- Feature list: JWKS cache + background refresh, single-flight Basic exchange, negative caching, disabled mode

### `crates/oidc-rs/README.md`

- Framework-agnostic core description
- Builder API example
- `Validator` and `BasicExchanger` usage

### `crates/oidc-rs-actix/README.md`

- Actix adapter description
- `AuthMiddleware` + `Authenticated` extractor example

---

## .gitignore

```
/target
```

---

## Verification plan

After extraction:

1. `cargo build` — both crates compile
2. `cargo test` — all unit + integration tests pass
3. `cargo clippy -- -D warnings` — no warnings
4. `cargo fmt --check` — formatting clean
5. `just check` — all of the above in one command

---

## Distribution model

Git dependency. Downstream projects consume via:

```toml
[dependencies]
oidc-rs = { git = "https://github.com/juspay/oidc-rs.git" }
oidc-rs-actix = { git = "https://github.com/juspay/oidc-rs.git" }
```

The `oidc-rs-actix` → `oidc-rs` path dependency resolves correctly within the workspace checkout. No crates.io publishing needed.
