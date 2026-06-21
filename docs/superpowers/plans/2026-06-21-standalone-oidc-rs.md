# Standalone oidc-rs + oidc-rs-actix Repo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the `oidc-rs` and `oidc-rs-actix` crates from Kronos PR #45 into a standalone repo, self-contained with inlined dependencies, READMEs, justfile, and git init.

**Architecture:** Cargo workspace with two members. `oidc-rs` is the framework-agnostic core (JWT validation + Basic→JWT exchange). `oidc-rs-actix` is the Actix-Web adapter (middleware + extractor + error mapping). Path dependency between them. Source code copied verbatim from PR; only Cargo.toml deps are inlined (not `workspace = true`).

**Tech Stack:** Rust 1.75+ MSRV, tokio, openidconnect 4, jsonwebtoken 10, actix-web 4, reqwest 0.12, dashmap 5

---

## File Structure

```
oidc-rs/
├── .gitignore
├── Cargo.toml                          ← workspace root
├── justfile
├── README.md
├── docs/superpowers/specs/             ← already exists
├── docs/superpowers/plans/             ← this file
├── crates/oidc-rs/
│   ├── Cargo.toml
│   ├── README.md
│   ├── src/{lib,config,identity,validator,exchanger}.rs
│   └── tests/fixtures/{extract_jwk.sh,test_rsa_priv.pem,test_rsa_n.txt,test_rsa_e.txt}
└── crates/oidc-rs-actix/
    ├── Cargo.toml
    ├── README.md
    ├── src/{lib,middleware,extractor,error}.rs
    └── tests/smoke.rs
```

Source files are copied verbatim from `/var/folders/x9/75w69q0j2md6j38j940c7kyh0000gn/T/opencode/kronos-pr45/crates/oidc-rs/` and `…/oidc-rs-actix/`.

---

### Task 1: Scaffold workspace + .gitignore

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`

- [ ] **Step 1: Write the workspace Cargo.toml**

```toml
[workspace]
members = ["crates/oidc-rs", "crates/oidc-rs-actix"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
```

- [ ] **Step 2: Write .gitignore**

```
/target
```

- [ ] **Step 3: Commit**

```bash
git init && git add -A && git commit -m "chore: scaffold workspace"
```

---

### Task 2: Create oidc-rs crate — Cargo.toml + source files

**Files:**
- Create: `crates/oidc-rs/Cargo.toml`
- Create: `crates/oidc-rs/src/lib.rs`
- Create: `crates/oidc-rs/src/config.rs`
- Create: `crates/oidc-rs/src/identity.rs`
- Create: `crates/oidc-rs/src/validator.rs`
- Create: `crates/oidc-rs/src/exchanger.rs`
- Create: `crates/oidc-rs/tests/fixtures/extract_jwk.sh`
- Create: `crates/oidc-rs/tests/fixtures/test_rsa_priv.pem`
- Create: `crates/oidc-rs/tests/fixtures/test_rsa_n.txt`
- Create: `crates/oidc-rs/tests/fixtures/test_rsa_e.txt`

- [ ] **Step 1: Write `crates/oidc-rs/Cargo.toml`**

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

- [ ] **Step 2: Copy source files verbatim from PR**

Copy these files from `/var/folders/x9/75w69q0j2md6j38j940c7kyh0000gn/T/opencode/kronos-pr45/crates/oidc-rs/src/` to `crates/oidc-rs/src/`:

- `lib.rs` — module declarations + re-exports
- `config.rs` — `AuthConfig`, `AuthConfigBuilder`, `BuildError`, `EnabledConfig`
- `identity.rs` — `Claims`, `Identity`, `AuthError`
- `validator.rs` — `Validator` with JWKS cache + background refresh
- `exchanger.rs` — `BasicExchanger` with cache, single-flight, negative-cache

Copy test fixtures from `…/tests/fixtures/` to `crates/oidc-rs/tests/fixtures/`:

- `extract_jwk.sh`
- `test_rsa_priv.pem`
- `test_rsa_n.txt`
- `test_rsa_e.txt`

- [ ] **Step 3: Verify build compiles**

Run: `cargo build -p oidc-rs`
Expected: compiles with no errors

- [ ] **Step 4: Verify tests pass**

Run: `cargo test -p oidc-rs`
Expected: all unit tests pass (config tests, identity tests, validator tests, exchanger tests)

- [ ] **Step 5: Commit**

```bash
git add crates/oidc-rs/ && git commit -m "feat(oidc-rs): framework-agnostic OIDC RS crate"
```

---

### Task 3: Create oidc-rs-actix crate — Cargo.toml + source files

**Files:**
- Create: `crates/oidc-rs-actix/Cargo.toml`
- Create: `crates/oidc-rs-actix/src/lib.rs`
- Create: `crates/oidc-rs-actix/src/middleware.rs`
- Create: `crates/oidc-rs-actix/src/extractor.rs`
- Create: `crates/oidc-rs-actix/src/error.rs`
- Create: `crates/oidc-rs-actix/tests/smoke.rs`

- [ ] **Step 1: Write `crates/oidc-rs-actix/Cargo.toml`**

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

- [ ] **Step 2: Copy source files verbatim from PR**

Copy these files from `/var/folders/x9/75w69q0j2md6j38j940c7kyh0000gn/T/opencode/kronos-pr45/crates/oidc-rs-actix/src/` to `crates/oidc-rs-actix/src/`:

- `lib.rs` — module declarations + re-exports
- `middleware.rs` — `AuthMiddleware`, `AuthState`, `AuthMode`, `resolve_identity`
- `extractor.rs` — `Authenticated(Identity)` extractor
- `error.rs` — `AuthError → HttpResponse` mapping

Copy integration test from `…/tests/` to `crates/oidc-rs-actix/tests/`:

- `smoke.rs`

- [ ] **Step 3: Verify build compiles**

Run: `cargo build -p oidc-rs-actix`
Expected: compiles with no errors

- [ ] **Step 4: Verify tests pass**

Run: `cargo test -p oidc-rs-actix`
Expected: smoke test passes (disabled mode injects `Identity::Disabled`)

- [ ] **Step 5: Commit**

```bash
git add crates/oidc-rs-actix/ && git commit -m "feat(oidc-rs-actix): Actix-Web adapter crate"
```

---

### Task 4: Write READMEs

**Files:**
- Create: `README.md`
- Create: `crates/oidc-rs/README.md`
- Create: `crates/oidc-rs-actix/README.md`

- [ ] **Step 1: Write top-level `README.md`**

Content:
- Project title + one-line description
- Two-crate structure overview
- Git dependency usage example (http URL: `https://github.com/juspay/oidc-rs.git`)
- Minimal usage example showing: `AuthConfig::builder()`, `Validator::new()`, `BasicExchanger::new()`, wrapping an Actix scope with `AuthMiddleware`, using `Authenticated` extractor in a handler
- Feature list: JWKS cache + background refresh, single-flight Basic exchange, negative caching, disabled mode, framework-agnostic core

- [ ] **Step 2: Write `crates/oidc-rs/README.md`**

Content:
- Framework-agnostic core description
- Builder API example (`AuthConfig::builder()`)
- `Validator` usage example
- `BasicExchanger` usage example
- Note: for Actix-Web users, see `oidc-rs-actix`

- [ ] **Step 3: Write `crates/oidc-rs-actix/README.md`**

Content:
- Actix-Web adapter description
- `AuthMiddleware` + `Authenticated` extractor example
- `AuthError → HttpResponse` mapping note
- Depends on `oidc-rs`

- [ ] **Step 4: Commit**

```bash
git add README.md crates/oidc-rs/README.md crates/oidc-rs-actix/README.md
git commit -m "docs: add READMEs"
```

---

### Task 5: Add justfile

**Files:**
- Create: `justfile`

- [ ] **Step 1: Write justfile**

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

- [ ] **Step 2: Verify justfile works**

Run: `just --list`
Expected: lists all recipes

- [ ] **Step 3: Commit**

```bash
git add justfile && git commit -m "chore: add justfile"
```

---

### Task 6: Final verification + clippy/fmt

- [ ] **Step 1: Run fmt check**

Run: `cargo fmt --check`
Expected: no diff (if diff, run `cargo fmt` and re-check)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: all tests pass across both crates

- [ ] **Step 4: Run `just check`**

Run: `just check`
Expected: fmt + clippy + test all pass

- [ ] **Step 5: Final commit if any formatting fixes**

```bash
git add -A && git commit -m "chore: fmt + clippy fixes" || echo "nothing to commit"
```
