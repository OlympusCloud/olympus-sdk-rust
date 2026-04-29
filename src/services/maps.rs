//! MapsService — geocoding, directions, and delivery-zone validation.
//!
//! Wraps the oc.maps.* endpoints served by the Olympus Commerce service (Rust)
//! via the Go API Gateway. The gateway holds the Google Maps API key so SDK
//! callers never embed it.
//!
//! Routes (all require tenant JWT auth, under `/api/v1/maps`):
//!
//!   * `POST /maps/geocode`
//!   * `POST /maps/directions`
//!   * `POST /maps/delivery-zones/validate`
//!
//! Added in v0.7.x (olympus-cloud-gcp#3227).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::Result;
use crate::http::OlympusHttpClient;

// ─────────────────────────────────────────────────────────── types ──

/// Request body for [`MapsService::geocode`].
#[derive(Debug, Clone, Serialize)]
pub struct GeocodeRequest {
    /// Free-text address to geocode.
    pub address: String,
}

/// Response from [`MapsService::geocode`].
#[derive(Debug, Clone, Deserialize)]
pub struct GeocodeResponse {
    /// Normalised formatted address returned by the geocoder.
    pub formatted: String,
    pub lat: f64,
    pub lng: f64,
    /// Google Maps place identifier — present when the geocoder resolves to a
    /// single place; `None` for ambiguous / partial matches.
    #[serde(default)]
    pub place_id: Option<String>,
}

/// A single navigation step inside [`DirectionsResponse::steps`].
#[derive(Debug, Clone, Deserialize)]
pub struct RouteStep {
    pub html_instructions: String,
    pub distance_text: String,
    pub duration_text: String,
}

/// Request body for [`MapsService::directions`].
#[derive(Debug, Clone, Serialize)]
pub struct DirectionsRequest {
    /// Origin address or `"lat,lng"` string.
    pub origin: String,
    /// Destination address or `"lat,lng"` string.
    pub destination: String,
    /// Travel mode: `"driving"` | `"walking"` | `"bicycling"` | `"transit"`.
    /// Defaults to `"driving"` server-side when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Response from [`MapsService::directions`].
#[derive(Debug, Clone, Deserialize)]
pub struct DirectionsResponse {
    pub distance_text: String,
    pub distance_meters: i64,
    pub duration_text: String,
    pub duration_seconds: i64,
    pub start_address: String,
    pub end_address: String,
    #[serde(default)]
    pub steps: Vec<RouteStep>,
}

/// Request body for [`MapsService::validate_delivery_zone`].
///
/// Either `(lat, lng)` or `address` must be provided.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ValidateDeliveryZoneRequest {
    /// Pre-geocoded latitude. Provide with `lng` to skip server-side geocoding.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    /// Pre-geocoded longitude.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lng: Option<f64>,
    /// Free-text address. Used when `lat`/`lng` are not provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Restrict zone lookup to a specific location. When `None` all active
    /// zones for the tenant are checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
}

/// Response from [`MapsService::validate_delivery_zone`].
#[derive(Debug, Clone, Deserialize)]
pub struct ValidateDeliveryZoneResponse {
    /// Whether the address falls inside any active delivery zone.
    pub in_zone: bool,
    /// Resolved (or echoed) latitude.
    pub lat: f64,
    /// Resolved (or echoed) longitude.
    pub lng: f64,
    /// UUID of the matched zone. `None` when `in_zone` is false.
    #[serde(default)]
    pub zone_id: Option<String>,
    #[serde(default)]
    pub zone_name: Option<String>,
    #[serde(default)]
    pub eta_minutes: Option<i32>,
    /// Delivery fee in smallest currency unit (cents / pence).
    #[serde(default)]
    pub delivery_fee_cents: Option<i64>,
    /// Minimum order amount in smallest currency unit.
    #[serde(default)]
    pub min_order_cents: Option<i64>,
    /// Formatted address from the geocoder when an address string was provided.
    #[serde(default)]
    pub formatted: Option<String>,
}

// ─────────────────────────────────────────────────────────── service ──

/// Maps and navigation services. Obtain via [`crate::OlympusClient::maps`].
pub struct MapsService {
    http: Arc<OlympusHttpClient>,
}

impl MapsService {
    /// Construct a new `MapsService` from a shared HTTP client.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Convert a free-text address into lat/lng coordinates.
    ///
    /// `POST /maps/geocode`
    pub async fn geocode(&self, request: GeocodeRequest) -> Result<GeocodeResponse> {
        let body = json!({ "address": request.address });
        let raw = self.http.post("/maps/geocode", &body).await?;
        serde_json::from_value(raw).map_err(crate::error::OlympusError::Json)
    }

    /// Return turn-by-turn directions between two addresses or coordinates.
    ///
    /// `POST /maps/directions`
    pub async fn directions(&self, request: DirectionsRequest) -> Result<DirectionsResponse> {
        let body = serde_json::to_value(&request).map_err(crate::error::OlympusError::Json)?;
        let raw = self.http.post("/maps/directions", &body).await?;
        serde_json::from_value(raw).map_err(crate::error::OlympusError::Json)
    }

    /// Check whether an address or coordinate pair is inside any of the
    /// tenant's active delivery zones.
    ///
    /// `POST /maps/delivery-zones/validate`
    pub async fn validate_delivery_zone(
        &self,
        request: ValidateDeliveryZoneRequest,
    ) -> Result<ValidateDeliveryZoneResponse> {
        let body = serde_json::to_value(&request).map_err(crate::error::OlympusError::Json)?;
        let raw = self.http.post("/maps/delivery-zones/validate", &body).await?;
        serde_json::from_value(raw).map_err(crate::error::OlympusError::Json)
    }
}
