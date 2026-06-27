# basic_server example

A minimal Axum server showing how to wire `oidc-rs-axum` middleware, extractors, and error handling.

## Run in disabled mode

No IdP required — all requests pass through with `Identity::Disabled`.

```sh
just example-axum-disabled
```

```sh
curl http://localhost:8080/whoami
# {"type":"disabled"}
```

## Run in enabled mode with Keycloak

The fastest way is the one-shot demo, which starts Keycloak, configures
clients, runs the server, exercises both credential paths, and tears down:

```sh
just demo-axum
```

To run the steps individually:

### 1. Start Keycloak

```sh
just keycloak-up
```

### 2. Configure clients and audience

```sh
just keycloak-setup
```

### 3. Start the example server

```sh
just example-axum
```

### 4. Test it

**Bearer (machine-to-machine via client_credentials):**

```sh
just example-bearer
```

**Basic (the library exchanges for you):**

```sh
just example-basic
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
just keycloak-down
```
