//! Framework-agnostic Authorization-header resolution: given a raw header
//! value, validate a Bearer JWT or exchange Basic credentials, returning
//! an [`Identity`].

use crate::{AuthError, BasicExchanger, Identity, Validator};
use base64::Engine as _;

/// Resolve an [`Identity`] from a raw `Authorization` header value.
///
/// - `None` (no header) returns [`AuthError::MissingHeader`].
/// - `Bearer <jwt>` — validates the JWT directly via `validator`.
/// - `Basic <base64(client_id:secret)>` — exchanges credentials via
///   `exchanger`, then validates the resulting JWT via `validator`.
/// - Unrecognised scheme or malformed value returns
///   [`AuthError::MalformedHeader`].
///
/// # Arguments
///
/// * `auth_header` — The raw `Authorization` header value (or `None` if
///   absent).
/// * `validator` — The JWT validator for signature/claims verification.
/// * `exchanger` — The Basic→JWT exchanger for `client_credentials` grant.
///
/// # Returns
///
/// `Ok(Identity::Bearer(claims))` or `Ok(Identity::Basic(claims))` on
/// success; `Err(AuthError)` on failure.
pub async fn resolve_identity(
    auth_header: Option<&str>,
    validator: &Validator,
    exchanger: &BasicExchanger,
) -> Result<Identity, AuthError> {
    let header = auth_header.ok_or(AuthError::MissingHeader)?;
    if let Some(token) = strip_scheme_prefix(header, "Bearer") {
        let claims = validator.validate(token).await?;
        Ok(Identity::Bearer(claims))
    } else if let Some(b64) = strip_scheme_prefix(header, "Basic") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|_| AuthError::MalformedHeader)?;
        let s = std::str::from_utf8(&decoded).map_err(|_| AuthError::MalformedHeader)?;
        let (id, secret) = s.split_once(':').ok_or(AuthError::MalformedHeader)?;
        let jwt = exchanger.exchange(id, secret).await?;
        let claims = validator.validate(&jwt).await?;
        Ok(Identity::Basic(claims))
    } else {
        Err(AuthError::MalformedHeader)
    }
}

/// Strip a case-insensitive auth-scheme prefix (RFC 7235 § 2.1). The scheme
/// token must be followed by a literal SP separator; surrounding whitespace
/// on the credential is left for the caller to trim.
///
/// Returns `None` when the header doesn't begin with `<scheme> ` regardless
/// of case, so e.g. `"bearer abc"`, `"BEARER abc"`, and `"Bearer abc"` all
/// match `strip_scheme_prefix(header, "Bearer")`, while `"BearerToken abc"`
/// (no space) and `"" / "Bearer"` (too short) do not.
fn strip_scheme_prefix<'a>(header: &'a str, scheme: &str) -> Option<&'a str> {
    let prefix_len = scheme.len();
    if header.len() <= prefix_len {
        return None;
    }
    if !header[..prefix_len].eq_ignore_ascii_case(scheme) {
        return None;
    }
    let rest = &header[prefix_len..];
    if !rest.starts_with(' ') {
        return None;
    }
    Some(rest.trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_scheme_prefix_matches_case_insensitively() {
        assert_eq!(strip_scheme_prefix("Bearer abc", "Bearer"), Some("abc"));
        assert_eq!(strip_scheme_prefix("bearer abc", "Bearer"), Some("abc"));
        assert_eq!(strip_scheme_prefix("BEARER abc", "Bearer"), Some("abc"));
        assert_eq!(
            strip_scheme_prefix("Basic dXNlcjpwYXNz", "Basic"),
            Some("dXNlcjpwYXNz")
        );
    }

    #[test]
    fn strip_scheme_prefix_rejects_no_space_after_scheme() {
        assert_eq!(strip_scheme_prefix("BearerToken abc", "Bearer"), None);
    }

    #[test]
    fn strip_scheme_prefix_rejects_too_short() {
        assert_eq!(strip_scheme_prefix("", "Bearer"), None);
        assert_eq!(strip_scheme_prefix("Bearer", "Bearer"), None);
    }

    #[test]
    fn strip_scheme_prefix_trims_leading_whitespace_after_scheme() {
        assert_eq!(strip_scheme_prefix("Bearer    abc", "Bearer"), Some("abc"));
    }
}
