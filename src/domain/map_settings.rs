use serde::{Deserialize, Serialize};

/// Map related settings shared by the web frontend and the desktop client.
///
/// `browser_tms_url` is used by the web map, `client_tms_url` by the desktop
/// client map, `wfs_url` is shared by both. The zoom limits constrain both the
/// raster source and the map view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSettings {
    pub browser_tms_url: String,
    pub client_tms_url: String,
    pub wfs_url: String,
    pub browser_min_zoom: i32,
    pub browser_max_zoom: i32,
    pub client_min_zoom: i32,
    pub client_max_zoom: i32,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub const DEFAULT_MIN_ZOOM: i32 = 0;
pub const DEFAULT_MAX_ZOOM: i32 = 19;

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            browser_tms_url: String::new(),
            client_tms_url: String::new(),
            wfs_url: String::new(),
            browser_min_zoom: DEFAULT_MIN_ZOOM,
            browser_max_zoom: DEFAULT_MAX_ZOOM,
            client_min_zoom: DEFAULT_MIN_ZOOM,
            client_max_zoom: DEFAULT_MAX_ZOOM,
            updated_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateMapSettingsRequest {
    #[serde(default)]
    pub browser_tms_url: Option<String>,
    #[serde(default)]
    pub client_tms_url: Option<String>,
    #[serde(default)]
    pub wfs_url: Option<String>,
    #[serde(default)]
    pub browser_min_zoom: Option<i32>,
    #[serde(default)]
    pub browser_max_zoom: Option<i32>,
    #[serde(default)]
    pub client_min_zoom: Option<i32>,
    #[serde(default)]
    pub client_max_zoom: Option<i32>,
}
