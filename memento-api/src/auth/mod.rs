/// Authentication middleware — Phase 2
///
/// Will implement:
/// - API key validation (SHA-256 hashed, stored in DB)
/// - Rate limiting per user/tier
/// - OAuth token exchange
#[allow(dead_code)]
pub struct AuthContext {
    pub user_id: String,
    pub plan: String,
}
