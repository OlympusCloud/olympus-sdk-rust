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
pub mod services;

pub use client::OlympusClient;
pub use config::OlympusConfig;
pub use constants::roles::OlympusRoles;
pub use constants::scopes::OlympusScopes;
pub use error::OlympusError;
