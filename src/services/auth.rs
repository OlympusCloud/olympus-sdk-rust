use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Authentication and user management service.
///
/// Wraps the Olympus Auth service (Rust, port 8001) via the Go API Gateway.
/// Routes: `/auth/*`, `/platform/users/*`.
pub struct AuthService {
    http: Arc<OlympusHttpClient>,
}

impl AuthService {
    /// Creates a new AuthService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Authenticates a user with email and password.
    pub async fn login(&self, email: &str, password: &str) -> Result<Value> {
        let body = json!({
            "email": email,
            "password": password,
        });
        self.http.post("/auth/login", &body).await
    }

    /// Registers a new user account.
    pub async fn register(&self, email: &str, password: &str, name: &str) -> Result<Value> {
        let body = json!({
            "email": email,
            "password": password,
            "name": name,
        });
        self.http.post("/auth/register", &body).await
    }

    /// Validates an access token and returns the associated user.
    pub async fn validate(&self, token: &str) -> Result<Value> {
        let body = json!({
            "token": token,
        });
        self.http.post("/auth/validate", &body).await
    }

    /// Exchanges a refresh token for a new token pair.
    pub async fn refresh(&self, refresh_token: &str) -> Result<Value> {
        let body = json!({
            "refresh_token": refresh_token,
        });
        self.http.post("/auth/refresh", &body).await
    }
}
