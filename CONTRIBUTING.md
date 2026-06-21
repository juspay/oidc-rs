# Contributing

Contributions are welcome! This project uses a standard fork-and-PR workflow.

## Development setup

```sh
git clone https://github.com/juspay/oidc-rs.git
cd oidc-rs
just build
just test
```

## Before opening a PR

All of the following must pass:

```sh
just fmt-check
just clippy
just test
```

CI runs the same commands, so running them locally saves a round-trip.

## Commit messages

Use [conventional commits](https://www.conventionalcommits.org/):

```
feat: add PKCE support
fix: handle expired JWKS cache correctly
docs: update validator README
ci: add cross-compile job
chore: bump dependencies
```

## Style

- Follow `rustfmt` defaults (`cargo fmt`).
- No comments unless they explain *why*, not *what*.
- Keep public API documentation (`///` doc comments) up to date.

## Licensing

By submitting a contribution you agree that it is dual-licensed under the
MIT and Apache-2.0 licenses, as described in the project README.
