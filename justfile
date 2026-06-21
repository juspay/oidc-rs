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

# --- Example server (crates/oidc-rs-actix/examples/basic_server.rs) ---

KC_IMAGE := "quay.io/keycloak/keycloak:26.0"
KC_NAME  := "oidc-rs-keycloak"
KC_PORT  := "8484"
KC_ADMIN := "admin"
KC_PASS  := "admin"
KC_REALM := "master"

# start a local Keycloak container for the example
keycloak-up:
    docker run -d --name {{KC_NAME}} \
        -p {{KC_PORT}}:8080 \
        -e KEYCLOAK_ADMIN={{KC_ADMIN}} \
        -e KEYCLOAK_ADMIN_PASSWORD={{KC_PASS}} \
        {{KC_IMAGE}} start-dev --http-port=8080
    @echo "Waiting for Keycloak to be ready..."
    @until curl -sf http://localhost:{{KC_PORT}}/realms/{{KC_REALM}}/.well-known/openid-configuration >/dev/null 2>&1; do \
        sleep 2; \
    done
    @echo "Keycloak is ready at http://localhost:{{KC_PORT}}"

# configure Keycloak clients and service accounts for the example
keycloak-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    KCADM="docker exec {{KC_NAME}} /opt/keycloak/bin/kcadm.sh --no-config --server http://localhost:8080 --user {{KC_ADMIN}} --password {{KC_PASS}}"
    $$KCADM create clients -r {{KC_REALM}} \
        -s clientId=my-api \
        -s 'redirectUris=["http://localhost:8080/callback"]' \
        -s publicClient=false \
        -s secret=my-api-secret \
        -s serviceAccountsEnabled=true \
        -s directAccessGrantsEnabled=true
    $$KCADM create clients -r {{KC_REALM}} \
        -s clientId=m2m-client \
        -s publicClient=false \
        -s secret=m2m-secret \
        -s serviceAccountsEnabled=true \
        -s directAccessGrantsEnabled=true
    echo "Clients created: my-api, m2m-client"

# stop and remove the Keycloak container
keycloak-down:
    -docker rm -f {{KC_NAME}}

# run the example server in enabled mode against local Keycloak
example:
    OIDC_ISSUER=http://localhost:{{KC_PORT}}/realms/{{KC_REALM}} \
    OIDC_AUDIENCES=my-api \
    cargo run -p oidc-rs-actix --example basic_server

# run the example server in disabled mode (no IdP required)
example-disabled:
    cargo run -p oidc-rs-actix --example basic_server

# call /whoami with a machine token from Keycloak (Bearer)
example-bearer:
    #!/usr/bin/env bash
    set -euo pipefail
    JWT=$$(curl -sf http://localhost:{{KC_PORT}}/realms/{{KC_REALM}}/protocol/openid-connect/token \
        -d grant_type=client_credentials \
        -d client_id=m2m-client \
        -d client_secret=m2m-secret | jq -r .access_token)
    curl -sf -H "Authorization: Bearer $$JWT" http://localhost:8080/whoami | jq .

# call /whoami with Basic credentials (library exchanges for you)
example-basic:
    curl -sf -u m2m-client:m2m-secret http://localhost:8080/whoami | jq .

# spin up Keycloak, configure clients, run the server, exercise both
# credential paths (Bearer + Basic), then tear down — all in one shot
demo:
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'just keycloak-down' EXIT
    just keycloak-up
    just keycloak-setup
    OIDC_ISSUER=http://localhost:{{KC_PORT}}/realms/{{KC_REALM}} \
    OIDC_AUDIENCES=my-api \
    cargo run -p oidc-rs-actix --example basic_server &
    SERVER_PID=$!
    # wait for the example server to bind
    until curl -sf http://localhost:8080/whoami >/dev/null 2>&1; do
        if ! kill -0 $$SERVER_PID 2>/dev/null; then
            echo "example server exited unexpectedly"; exit 1
        fi
        sleep 1
    done
    echo ""
    echo "=== Bearer (client_credentials JWT) ==="
    just example-bearer
    echo ""
    echo "=== Basic (library exchanges for you) ==="
    just example-basic
    echo ""
    kill $$SERVER_PID 2>/dev/null || true
    wait $$SERVER_PID 2>/dev/null || true
