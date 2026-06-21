# basic_server example

A minimal Actix-Web server showing how to wire `oidc-rs-actix` middleware, extractors, and error handling.

## Run in disabled mode

No IdP required — all requests pass through with `Identity::Disabled`.

```sh
cargo run -p oidc-rs-actix --example basic_server
```

```sh
curl http://localhost:8080/whoami
# {"type":"disabled"}
```

## Run in enabled mode with Keycloak

### 1. Start Keycloak

```sh
docker run -d \
  --name keycloak \
  -p 8484:8080 \
  -e KEYCLOAK_ADMIN=admin \
  -e KEYCLOAK_ADMIN_PASSWORD=admin \
  quay.io/keycloak/keycloak:26.0 \
  start-dev \
  --http-port=8080
```

Wait for Keycloak to be ready (usually 10–15 seconds):

```sh
until curl -sf http://localhost:8484/realms/master/.well-known/openid-configuration >/dev/null; do
  echo "waiting for Keycloak..."; sleep 2
done
```

### 2. Configure a client and audience

Using `kcadm` (shipped inside the Keycloak container):

```sh
# Create a client for the example API
docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  create clients \
  -r master \
  -s clientId=my-api \
  -s "redirectUris=[\"http://localhost:8080/callback\"]" \
  -s publicClient=false \
  -s secret=my-api-secret \
  -s serviceAccountsEnabled=true \
  -s directAccessGrantsEnabled=true \
  --no-config \
  --server http://localhost:8080 \
  --user admin \
  --password admin

# Create a client for a machine caller
docker exec keycloak /opt/keycloak/bin/kcadm.sh \
  create clients \
  -r master \
  -s clientId=m2m-client \
  -s publicClient=false \
  -s secret=m2m-secret \
  -s serviceAccountsEnabled=true \
  -s directAccessGrantsEnabled=true \
  --no-config \
  --server http://localhost:8080 \
  --user admin \
  --password admin
```

### 3. Start the example server

```sh
OIDC_ISSUER=http://localhost:8484/realms/master \
OIDC_AUDIENCES=my-api \
cargo run -p oidc-rs-actix --example basic_server
```

### 4. Test it

**Bearer (machine-to-machine via client_credentials):**

```sh
# Exchange client credentials for a JWT
JWT=$(curl -sf http://localhost:8484/realms/master/protocol/openid-connect/token \
  -d grant_type=client_credentials \
  -d client_id=m2m-client \
  -d client_secret=m2m-secret \
  | jq -r .access_token)

# Call the protected endpoint
curl -sf -H "Authorization: Bearer $JWT" http://localhost:8080/whoami
```

**Basic (the library exchanges for you):**

```sh
curl -sf -u m2m-client:m2m-secret http://localhost:8080/whoami
```

**Missing/invalid credentials:**

```sh
curl -i http://localhost:8080/whoami
# HTTP/1.1 401 Unauthorized
# {"error":{"code":"MISSING_AUTHORIZATION","message":"missing Authorization header"}}
```

## Endpoints

| Method | Path        | Description |
|--------|-------------|-------------|
| GET    | `/whoami`   | Returns the authenticated `Identity` as JSON |
| GET    | `/protected` | Returns a greeting using `claims.sub` |

## Cleanup

```sh
docker rm -f keycloak
```
