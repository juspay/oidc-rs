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
    cargo clippy --all-targets -- -D warnings

# check formatting
fmt-check:
    cargo fmt --check

# apply formatting
fmt:
    cargo fmt

# run all checks (fmt + clippy + test)
check: fmt-check clippy test
