use crate::domain::map_settings::{
    normalize_layers, normalize_srs, validate_layers, validate_wfs, MapSettings,
    UpdateMapSettingsRequest,
};
use crate::error::Result;
use std::sync::Arc;

/// Serves the map settings that are shared between the web frontend and the
/// desktop client.
///
/// The table holds a single row; when it is missing the defaults (empty layer
/// lists) are returned so that callers never have to handle a missing row.
pub struct MapSettingsService {
    repo: Arc<crate::infrastructure::DbRepository>,
}

impl MapSettingsService {
    pub fn new(repo: Arc<crate::infrastructure::DbRepository>) -> Self {
        Self { repo }
    }

    pub async fn get(&self) -> Result<MapSettings> {
        Ok(self.repo.get_map_settings().await.unwrap_or_default())
    }

    pub async fn update(&self, req: UpdateMapSettingsRequest) -> Result<MapSettings> {
        let current = self.get().await?;

        let mut browser_layers = req.browser_layers.unwrap_or(current.browser_layers);
        let mut client_layers = req.client_layers.unwrap_or(current.client_layers);
        let mut wfs = req.wfs.unwrap_or(current.wfs);

        validate_layers(&browser_layers)?;
        validate_layers(&client_layers)?;
        validate_wfs(&wfs)?;

        normalize_layers(&mut browser_layers);
        normalize_layers(&mut client_layers);
        wfs.srs = normalize_srs(&wfs.srs);

        self.repo
            .upsert_map_settings(&browser_layers, &client_layers, &wfs)
            .await?;

        self.get().await
    }
}
