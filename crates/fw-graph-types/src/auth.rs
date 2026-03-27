//! JWT authentication types — decoded claims attached to every request context.
//!
//! fw-graph-serv validates the JWT and produces these.
//! All other packages consume them read-only.

/// Decoded Supabase JWT claims, attached to every request context.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JwtClaims {
  pub sub:   uuid::Uuid,
  pub role:  JwtRole,
  pub email: Option<String>,
  pub exp:   i64,
  /// Raw claims for forwarding via set_config to Postgres RLS.
  pub raw:   serde_json::Value,
}

/// The role extracted from the JWT — determines Postgres role for RLS.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JwtRole {
  Anon,
  Authenticated,
  ServiceRole,
  Custom(String),
}

impl JwtRole {
  pub fn as_str(&self) -> &str {
    match self {
      JwtRole::Anon          => "anon",
      JwtRole::Authenticated => "authenticated",
      JwtRole::ServiceRole   => "service_role",
      JwtRole::Custom(s)     => s.as_str(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn jwt_role_as_str() {
    assert_eq!(JwtRole::Anon.as_str(), "anon");
    assert_eq!(JwtRole::Authenticated.as_str(), "authenticated");
    assert_eq!(JwtRole::ServiceRole.as_str(), "service_role");
    assert_eq!(JwtRole::Custom("admin".to_string()).as_str(), "admin");
  }
}
