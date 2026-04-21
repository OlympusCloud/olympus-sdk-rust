//! SmartHomeService — consumer smart-home integration.
//!
//! Covers platforms (Hue, SmartThings, HomeKit, Google Home), devices, rooms,
//! scenes, and automations. All calls are tenant-scoped via the user's JWT.
//!
//! Routes: `/smart-home/*`.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Filter options for [`SmartHomeService::list_devices`].
#[derive(Default, Debug, Clone)]
pub struct ListDevicesOptions<'a> {
    pub platform_id: Option<&'a str>,
    pub room_id: Option<&'a str>,
}

/// Smart home integration: platforms, devices, rooms, scenes, and automations.
pub struct SmartHomeService {
    http: Arc<OlympusHttpClient>,
}

impl SmartHomeService {
    /// Creates a new SmartHomeService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // -----------------------------------------------------------------------
    // Platforms
    // -----------------------------------------------------------------------

    /// List connected smart home platforms (Hue, SmartThings, HomeKit, etc.).
    pub async fn list_platforms(&self) -> Result<Value> {
        self.http.get("/smart-home/platforms").await
    }

    // -----------------------------------------------------------------------
    // Devices
    // -----------------------------------------------------------------------

    /// List all smart home devices across connected platforms.
    pub async fn list_devices(&self, opts: ListDevicesOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.platform_id {
            query.push(("platform_id", p.to_string()));
        }
        if let Some(r) = opts.room_id {
            query.push(("room_id", r.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
        if query_refs.is_empty() {
            self.http.get("/smart-home/devices").await
        } else {
            self.http
                .get_with_query("/smart-home/devices", &query_refs)
                .await
        }
    }

    /// Get details for a single smart home device.
    pub async fn get_device(&self, device_id: &str) -> Result<Value> {
        let path = format!("/smart-home/devices/{}", urlencoding::encode(device_id));
        self.http.get(&path).await
    }

    /// Send a control command to a device (on/off, brightness, color, etc.).
    pub async fn control_device(&self, device_id: &str, command: Value) -> Result<Value> {
        let path = format!(
            "/smart-home/devices/{}/control",
            urlencoding::encode(device_id)
        );
        self.http.post(&path, &command).await
    }

    // -----------------------------------------------------------------------
    // Rooms
    // -----------------------------------------------------------------------

    /// List rooms with their associated devices.
    pub async fn list_rooms(&self) -> Result<Value> {
        self.http.get("/smart-home/rooms").await
    }

    // -----------------------------------------------------------------------
    // Scenes (v0.3.0 — Issue #2569)
    // -----------------------------------------------------------------------

    /// List automation scenes (e.g. "Good morning", "Movie night").
    pub async fn list_scenes(&self) -> Result<Value> {
        self.http.get("/smart-home/scenes").await
    }

    /// Activate a scene by ID.
    pub async fn activate_scene(&self, scene_id: &str) -> Result<Value> {
        let path = format!(
            "/smart-home/scenes/{}/activate",
            urlencoding::encode(scene_id)
        );
        self.http.post(&path, &json!({})).await
    }

    /// Create a new scene with devices and actions.
    pub async fn create_scene(&self, scene: Value) -> Result<Value> {
        self.http.post("/smart-home/scenes", &scene).await
    }

    /// Delete a scene.
    pub async fn delete_scene(&self, scene_id: &str) -> Result<()> {
        let path = format!("/smart-home/scenes/{}", urlencoding::encode(scene_id));
        self.http.delete(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Automations
    // -----------------------------------------------------------------------

    /// List automation rules (trigger-action).
    pub async fn list_automations(&self) -> Result<Value> {
        self.http.get("/smart-home/automations").await
    }

    /// Create a new automation rule.
    pub async fn create_automation(&self, automation: Value) -> Result<Value> {
        self.http.post("/smart-home/automations", &automation).await
    }

    /// Delete an automation rule.
    pub async fn delete_automation(&self, automation_id: &str) -> Result<()> {
        let path = format!(
            "/smart-home/automations/{}",
            urlencoding::encode(automation_id)
        );
        self.http.delete(&path).await?;
        Ok(())
    }
}
