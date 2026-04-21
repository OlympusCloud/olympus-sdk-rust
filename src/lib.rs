//! Official Rust SDK for [Olympus Cloud](https://olympuscloud.ai).
//!
//! Provides typed async access to all platform services.
//!
//! ```rust,no_run
//! use olympus_sdk::OlympusClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = OlympusClient::new("com.my-app", "oc_live_...");
//!     let items = client.commerce().list_orders(None).await?;
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod config;
pub mod constants;
pub mod error;
pub mod http;
pub mod identity;
pub mod services;
pub mod session;
pub mod silent_refresh;
pub mod tenant;

pub use client::OlympusClient;
pub use config::OlympusConfig;
pub use constants::roles::OlympusRoles;
pub use constants::scopes::OlympusScopes;
pub use error::OlympusError;
pub use session::{AuthSession, SessionEvent};
pub use silent_refresh::SilentRefreshHandle;

// Re-export the tenant + identity API types at the crate root so apps can
// write `use olympus_sdk::{TenantApi, IdentityApi}` without reaching into
// submodules. Named re-exports (not `pub use tenant::*`) to avoid any
// future collision with `crate::services::identity`.
pub use identity::{
    IdentityApi, InviteCreateRequest, InviteHandle, InviteStatus, RemoveFromTenantResponse,
};
pub use tenant::{
    AppInstall, ExchangedSession, Tenant, TenantApi, TenantCreateRequest, TenantFirstAdmin,
    TenantOption, TenantProvisionResult, TenantUpdate,
};
