use std::sync::Arc;

use crate::config::OlympusConfig;
use crate::error::Result;
use crate::http::OlympusHttpClient;
use crate::services::ai::AiService;
use crate::services::auth::AuthService;
use crate::services::business::BusinessService;
use crate::services::commerce::CommerceService;
use crate::services::creator::CreatorService;
use crate::services::platform::PlatformService;
use crate::services::pos::PosService;

/// Main entry point for the Olympus Cloud SDK.
///
/// Provides typed async access to all platform services via lazy-initialized
/// service accessors.
///
/// # Example
///
/// ```rust,no_run
/// use olympus_sdk::OlympusClient;
///
/// #[tokio::main]
/// async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
///     let client = OlympusClient::new("com.my-app", "oc_live_...");
///     let orders = client.commerce().list_orders(None).await?;
///     Ok(())
/// }
/// ```
pub struct OlympusClient {
    http: Arc<OlympusHttpClient>,
}

impl OlympusClient {
    /// Creates a new client with the given app_id and api_key, using production defaults.
    pub fn new(app_id: impl Into<String>, api_key: impl Into<String>) -> Self {
        let config = OlympusConfig::new(app_id, api_key);
        Self::from_config(config)
    }

    /// Creates a new client from an explicit configuration.
    pub fn from_config(config: OlympusConfig) -> Self {
        let http = OlympusHttpClient::new(Arc::new(config))
            .expect("failed to build HTTP client");
        Self {
            http: Arc::new(http),
        }
    }

    /// Returns a new client, or an error if the HTTP client cannot be constructed.
    pub fn try_from_config(config: OlympusConfig) -> Result<Self> {
        let http = OlympusHttpClient::new(Arc::new(config))?;
        Ok(Self {
            http: Arc::new(http),
        })
    }

    /// Returns the authentication service.
    pub fn auth(&self) -> AuthService {
        AuthService::new(Arc::clone(&self.http))
    }

    /// Returns the commerce/orders service.
    pub fn commerce(&self) -> CommerceService {
        CommerceService::new(Arc::clone(&self.http))
    }

    /// Returns the AI inference and agent service.
    pub fn ai(&self) -> AiService {
        AiService::new(Arc::clone(&self.http))
    }

    /// Returns the creator platform service.
    pub fn creator(&self) -> CreatorService {
        CreatorService::new(Arc::clone(&self.http))
    }

    /// Returns the tenant lifecycle (platform) service.
    pub fn platform(&self) -> PlatformService {
        PlatformService::new(Arc::clone(&self.http))
    }

    /// Returns the business data access service.
    pub fn business(&self) -> BusinessService {
        BusinessService::new(Arc::clone(&self.http))
    }

    /// Returns the POS voice order integration service.
    pub fn pos(&self) -> PosService {
        PosService::new(Arc::clone(&self.http))
    }
}
