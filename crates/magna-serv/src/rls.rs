//! RLS context — set Postgres session variables for Row Level Security.

use crate::error::ServError;
use magna_types::JwtClaims;
use sqlx::PgConnection;

/// Apply JWT claims as Postgres session configuration for RLS.
///
/// Executes `set_config` for the three keys Supabase RLS policies read:
/// - `request.jwt.claims` — full JSON of the decoded token
/// - `request.jwt.sub`    — the user UUID as a string
/// - `role`               — the Postgres role to apply
///
/// All settings are transaction-local (`is_local = true`).
pub async fn apply_rls_context(
    conn: &mut PgConnection,
    claims: &JwtClaims,
) -> Result<(), ServError> {
    let claims_json = serde_json::to_string(&claims.raw)
        .map_err(|e| ServError::ConfigError(e.to_string()))?;
    let sub_str = claims.sub.to_string();
    let role_str = claims.role.as_str().to_owned();

    sqlx::query(
        "SELECT \
            set_config('request.jwt.claims', $1, true), \
            set_config('request.jwt.sub', $2, true), \
            set_config('role', $3, true)",
    )
    .bind(&claims_json)
    .bind(&sub_str)
    .bind(&role_str)
    .execute(conn)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use magna_types::{JwtClaims, JwtRole};
    use uuid::Uuid;

    #[test]
    fn claims_json_serialization_is_valid() {
        let claims = JwtClaims {
            sub: Uuid::nil(),
            role: JwtRole::Authenticated,
            email: Some("test@example.com".to_string()),
            exp: 9_999_999_999,
            raw: serde_json::json!({
                "sub":   "00000000-0000-0000-0000-000000000000",
                "role":  "authenticated",
                "email": "test@example.com",
                "exp":   9_999_999_999_i64,
            }),
        };

        // Verify that raw JSON serializes without error (mirrors what apply_rls_context does).
        let json_str = serde_json::to_string(&claims.raw)
            .expect("should serialize");
        assert!(json_str.contains("authenticated"));
        assert!(json_str.contains("test@example.com"));
    }
}
