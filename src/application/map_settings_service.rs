use std::sync::Arc;
use crate::domain::map_settings::{MapSettings, UpdateMapSettingsRequest};
use crate::error::{AppError, Result};

/// Serves the map settings that are shared between the web frontend and the
/// desktop client.
///
/// The table holds a single row; when it is missing the defaults (empty
/// strings, zoom 0..19) are returned so that callers never have to handle a
/// missing row.
pub struct MapSettingsService {
    repo: Arc<crate::infrastructure::DbRepository>,
}

fn validate_zoom(min: i32, max: i32, label: &str) -> Result<()> {
    if !(0..=24).contains(&min) || !(0..=24).contains(&max) {
        return Err(AppError::BadRequest(format!("{label} zoom must be between 0 and 24")));
    }
    if min > max {
        return Err(AppError::BadRequest(format!("{label} min zoom must not exceed max zoom")));
    }
    Ok(())
}

impl MapSettingsService {
    pub fn new(repo: Arc<crate::infrastructure::DbRepository>) -> Self { Self { repo } }

    pub async fn get(&self) -> Result<MapSettings> {
        Ok(self.repo.get_map_settings().await.unwrap_or_default())
    }

    pub async fn update(&self, req: UpdateMapSettingsRequest) -> Result<MapSettings> {
        let current = self.get().await?;
        let browser_tms_url = req.browser_tms_url.unwrap_or(current.browser_tms_url);
        let client_tms_url = req.client_tms_url.unwrap_or(current.client_tms_url);
        let wfs_url = req.wfs_url.unwrap_or(current.wfs_url);
        let browser_min_zoom = req.browser_min_zoom.unwrap_or(current.browser_min_zoom);
        let browser_max_zoom = req.browser_max_zoom.unwrap_or(current.browser_max_zoom);
        let client_min_zoom = req.client_min_zoom.unwrap_or(current.client_min_zoom);
        let client_max_zoom = req.client_max_zoom.unwrap_or(current.client_max_zoom);

        validate_zoom(browser_min_zoom, browser_max_zoom, "browser")?;
        validate_zoom(client_min_zoom, client_max_zoom, "client")?;

        self.repo
            .upsert_map_settings(
                &browser_tms_url,
                &client_tms_url,
                &wfs_url,
                browser_min_zoom,
                browser_max_zoom,
                client_min_zoom,
                client_max_zoom,
            )
            .await?;

        self.get().await
    }
}
