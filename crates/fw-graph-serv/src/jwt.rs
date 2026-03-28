//! JWT authentication — decode and validate Supabase JWTs.

use crate::error::ServError;
use fw_graph_types::{JwtClaims, JwtRole};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use uuid::Uuid;

/// Strip "Bearer " prefix from an Authorization header value.
///
/// Returns the raw token string, or `None` if the input is `None`.
pub fn extract_bearer_token(header: Option<&str>) -> Option<&str> {
    let h = header?;
    if let Some(token) = h.strip_prefix("Bearer ") {
        Some(token)
    } else {
        Some(h)
    }
}

/// Decode and validate a JWT, returning typed [`JwtClaims`].
///
/// - `token = None` → anonymous claims (nil UUID, [`JwtRole::Anon`]).
/// - Valid token → decoded claims with sub/role/email/exp.
/// - Invalid or expired token → [`ServError::JwtError`].
///
/// If the token contains no `role` claim the `default_role` parameter is used,
/// falling back to `"anon"` if that is also `None`.
pub fn decode_jwt(
    token: Option<&str>,
    secret: &str,
    default_role: Option<&str>,
) -> Result<JwtClaims, ServError> {
    let Some(token) = token else {
        // No token → anonymous
        let role_str = default_role.unwrap_or("anon");
        let role = parse_role(role_str);
        return Ok(JwtClaims {
            sub: Uuid::nil(),
            role,
            email: None,
            exp: 0,
            raw: serde_json::Value::Object(Default::default()),
        });
    };

    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    // We validate expiry ourselves via the standard exp claim; allow the library
    // to also check it by keeping validate_exp = true (the default).
    validation.validate_exp = true;

    let token_data = decode::<serde_json::Value>(token, &key, &validation)
        .map_err(|e| ServError::JwtError(e.to_string()))?;

    let claims_val = token_data.claims;

    let sub: Uuid = claims_val
        .get("sub")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or(Uuid::nil());

    let role_str = claims_val
        .get("role")
        .and_then(|v| v.as_str())
        .or(default_role)
        .unwrap_or("anon");
    let role = parse_role(role_str);

    let email = claims_val
        .get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    let exp = claims_val
        .get("exp")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    Ok(JwtClaims {
        sub,
        role,
        email,
        exp,
        raw: claims_val,
    })
}

fn parse_role(role: &str) -> JwtRole {
    match role {
        "anon" => JwtRole::Anon,
        "authenticated" => JwtRole::Authenticated,
        "service_role" => JwtRole::ServiceRole,
        other => JwtRole::Custom(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    const SECRET: &str = "test-secret-at-least-32-bytes-long!!";

    fn make_token(claims: &serde_json::Value) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .expect("encode should succeed")
    }

    fn future_exp() -> i64 {
        // 1 hour from a fixed recent timestamp (avoids clock dependency in tests)
        2_000_000_000_i64 // year 2033 — well in the future
    }

    #[test]
    fn decode_valid_jwt_returns_correct_claims() {
        let user_id = "550e8400-e29b-41d4-a716-446655440000";
        let claims = serde_json::json!({
            "sub":   user_id,
            "role":  "authenticated",
            "email": "shane@example.com",
            "exp":   future_exp(),
        });
        let token = make_token(&claims);

        let result = decode_jwt(Some(&token), SECRET, None).expect("should decode");

        assert_eq!(result.sub.to_string(), user_id);
        assert!(matches!(result.role, JwtRole::Authenticated));
        assert_eq!(result.email.as_deref(), Some("shane@example.com"));
        assert_eq!(result.exp, future_exp());
    }

    #[test]
    fn no_token_returns_anon_claims() {
        let result = decode_jwt(None, SECRET, None).expect("should succeed");

        assert_eq!(result.sub, Uuid::nil());
        assert!(matches!(result.role, JwtRole::Anon));
        assert!(result.email.is_none());
    }

    #[test]
    fn invalid_token_returns_error() {
        let result = decode_jwt(Some("not.a.valid.jwt"), SECRET, None);

        assert!(result.is_err());
        match result {
            Err(ServError::JwtError(_)) => {}
            other => panic!("expected JwtError, got {:?}", other),
        }
    }

    #[test]
    fn expired_token_returns_error() {
        let claims = serde_json::json!({
            "sub":  "550e8400-e29b-41d4-a716-446655440000",
            "role": "authenticated",
            "exp":  1_000_000_i64, // year 1970 — long expired
        });
        let token = make_token(&claims);

        let result = decode_jwt(Some(&token), SECRET, None);

        assert!(result.is_err());
        match result {
            Err(ServError::JwtError(_)) => {}
            other => panic!("expected JwtError for expired token, got {:?}", other),
        }
    }

    #[test]
    fn default_role_parameter_is_used_when_no_role_in_token() {
        let claims = serde_json::json!({
            "sub": "550e8400-e29b-41d4-a716-446655440000",
            "exp": future_exp(),
            // no "role" field
        });
        let token = make_token(&claims);

        let result = decode_jwt(Some(&token), SECRET, Some("service_role"))
            .expect("should decode");

        assert!(matches!(result.role, JwtRole::ServiceRole));
    }

    #[test]
    fn extract_bearer_token_strips_prefix_and_handles_raw() {
        // With "Bearer " prefix
        assert_eq!(
            extract_bearer_token(Some("Bearer mytoken123")),
            Some("mytoken123")
        );
        // Without prefix (raw token)
        assert_eq!(
            extract_bearer_token(Some("mytoken123")),
            Some("mytoken123")
        );
        // None input
        assert_eq!(extract_bearer_token(None), None);
    }
}
