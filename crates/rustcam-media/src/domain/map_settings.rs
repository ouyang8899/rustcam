use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// Map related settings shared by the web frontend and the desktop client.
///
/// `browser_layers` / `client_layers` hold the raster basemap layers for the
/// web map and the desktop client map respectively. Every layer carries its
/// own `srs` (the projection of the tile service) so that each renderer can
/// decide whether it is able to consume it.
///
/// `wfs` is shared by both: its `srs` is the SRS the WFS server should render
/// the point features in (passed as `srsName`).
///
/// There is deliberately no end-level zoom configuration: every renderer
/// derives its view zoom range from the layer it actually displays, so 2D and
/// 3D views of the same end can follow different imagery without conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSettings {
    pub browser_layers: Vec<MapLayer>,
    pub client_layers: Vec<MapLayer>,
    pub wfs: WfsSettings,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Which renderer a layer is meant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerDim {
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
}

impl LayerDim {
    pub fn as_str(&self) -> &'static str {
        match self {
            LayerDim::TwoD => "2d",
            LayerDim::ThreeD => "3d",
        }
    }
}

/// A single raster basemap layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapLayer {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub dim: LayerDim,
    pub url: String,
    pub srs: String,
    /// Tile access protocol; decides how the row number is interpreted.
    /// See [`PROTOCOLS`]. Empty means "detect it from the url".
    #[serde(default)]
    pub protocol: String,
    #[serde(default)]
    pub format: Option<String>,
    /// Tile size in pixels (256 or 512). Only needed when the tile service
    /// serves 512px tiles; 256 is the default for most services.
    #[serde(default)]
    pub tile_size: Option<u32>,
    /// Bing Maps API key. Required by the official Bing drivers (Cesium
    /// `BingMapsImageryProvider`, osgEarth `BingImageLayer`) used by the 3D
    /// renderers; the 2D renderers use the keyless quadkey template instead.
    #[serde(default)]
    pub bing_key: Option<String>,
    /// Bing imagery set: `Aerial` (default) or `Road`.
    #[serde(default)]
    pub imagery_set: Option<String>,
    #[serde(default = "default_min_zoom")]
    pub min_zoom: i32,
    #[serde(default = "default_max_zoom")]
    pub max_zoom: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// WFS point service, shared by the web map and the desktop client.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WfsSettings {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub srs: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateMapSettingsRequest {
    #[serde(default)]
    pub browser_layers: Option<Vec<MapLayer>>,
    #[serde(default)]
    pub client_layers: Option<Vec<MapLayer>>,
    #[serde(default)]
    pub wfs: Option<WfsSettings>,
}

pub const DEFAULT_MIN_ZOOM: i32 = 0;
pub const DEFAULT_MAX_ZOOM: i32 = 19;
pub const MAX_ZOOM_LIMIT: i32 = 24;
/// Tile size assumed when a layer does not say otherwise.
pub const DEFAULT_TILE_SIZE: u32 = 256;
/// Bing imagery set used when a layer does not say otherwise.
pub const DEFAULT_IMAGERY_SET: &str = "Aerial";

/// Tile access protocols a layer can be configured with.
///
/// The protocol describes *how the tile address is built*, i.e. how the row
/// number has to be interpreted:
///
/// - `xyz`     – standard grid, `{x}` / `{y}` / `{z}`, row counted from the top
/// - `tms`     – same grid but the row is counted from the bottom (needs a flip)
/// - `wmts`    – WMTS `GetTile` parameters (`TILEMATRIX` / `TILEROW` / `TILECOL`)
/// - `quadkey` – Bing style single `{quadkey}` token
///
/// Engines pick the matching imagery provider / tile layer by protocol, and a
/// protocol that an engine cannot handle is skipped with an explicit error.
pub const PROTOCOL_XYZ: &str = "xyz";
pub const PROTOCOL_TMS: &str = "tms";
pub const PROTOCOL_WMTS: &str = "wmts";
pub const PROTOCOL_QUADKEY: &str = "quadkey";
pub const PROTOCOLS: &[&str] = &[PROTOCOL_XYZ, PROTOCOL_TMS, PROTOCOL_WMTS, PROTOCOL_QUADKEY];

/// Guess the protocol from the url structure.
pub fn detect_protocol(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("{quadkey}") {
        return PROTOCOL_QUADKEY;
    }
    if lower.contains("tilematrix") && lower.contains("tilerow") && lower.contains("tilecol") {
        return PROTOCOL_WMTS;
    }
    if lower.contains("{-y}") {
        return PROTOCOL_TMS;
    }
    PROTOCOL_XYZ
}

/// Does the url fit the placeholder convention of the protocol?
pub fn protocol_matches_url(url: &str, protocol: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    match protocol.trim().to_ascii_lowercase().as_str() {
        PROTOCOL_QUADKEY => lower.contains("{quadkey}"),
        PROTOCOL_WMTS => {
            lower.contains("tilematrix") && lower.contains("tilerow") && lower.contains("tilecol")
        }
        // xyz 与 tms 使用同一套占位符，区别只在行号方向；
        // {-y} 是我们表示「行号自下而上」的标记，同样接受。
        PROTOCOL_XYZ | PROTOCOL_TMS => {
            lower.contains("{x}")
                && lower.contains("{z}")
                && (lower.contains("{y}") || lower.contains("{-y}"))
        }
        _ => false,
    }
}

/// Does this protocol require the row number to be flipped?
pub fn protocol_flips_y(protocol: &str) -> bool {
    protocol.trim().to_ascii_lowercase() == PROTOCOL_TMS
}

/// The placeholder a protocol requires, for error messages.
pub fn protocol_placeholder_hint(protocol: &str) -> &'static str {
    match protocol.trim().to_ascii_lowercase().as_str() {
        PROTOCOL_QUADKEY => "{quadkey}",
        PROTOCOL_WMTS => "TILEMATRIX/TILEROW/TILECOL",
        _ => "{x}/{y}/{z}",
    }
}

/// Guess the image format from the url extension.
pub fn detect_format(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    let path = lower.split('?').next().unwrap_or("");
    if path.ends_with(".png") {
        Some("png".to_string())
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        Some("jpeg".to_string())
    } else if path.ends_with(".webp") {
        Some("webp".to_string())
    } else {
        None
    }
}

fn default_min_zoom() -> i32 {
    DEFAULT_MIN_ZOOM
}

fn default_max_zoom() -> i32 {
    DEFAULT_MAX_ZOOM
}

fn default_true() -> bool {
    true
}

impl Default for MapSettings {
    fn default() -> Self {
        Self {
            browser_layers: Vec::new(),
            client_layers: Vec::new(),
            wfs: WfsSettings::default(),
            updated_at: chrono::Utc::now(),
        }
    }
}

/// Canonicalize an SRS string: trim whitespace and uppercase the `epsg:` prefix.
///
/// Anything that is not a plain `EPSG:<digits>` code (PROJ strings, WKT) is
/// returned trimmed but otherwise untouched, so that osgEarth can still parse
/// it on the client side.
pub fn normalize_srs(srs: &str) -> String {
    let s = srs.trim();
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("epsg:") {
        let digits = rest.trim();
        if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
            return format!("EPSG:{}", digits);
        }
    }
    s.to_string()
}

/// Validate a layer list: every layer needs a tile url, a supported provider,
/// an SRS and a sane zoom range.
pub fn validate_layers(layers: &[MapLayer]) -> Result<()> {
    for (index, layer) in layers.iter().enumerate() {
        let label = format!("图层 {}", index + 1);
        if layer.url.trim().is_empty() {
            return Err(AppError::BadRequest(format!(
                "{} 的瓦片地址不能为空",
                label
            )));
        }
        if layer.srs.trim().is_empty() {
            return Err(AppError::BadRequest(format!("{} 的 SRS 不能为空", label)));
        }
        let protocol = layer.protocol.trim().to_ascii_lowercase();
        if !protocol.is_empty() {
            if !PROTOCOLS.contains(&protocol.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "{} 的瓦片协议 {} 不受支持（可用：{}）",
                    label,
                    protocol,
                    PROTOCOLS.join(" / ")
                )));
            }
            if !protocol_matches_url(&layer.url, &protocol) {
                return Err(AppError::BadRequest(format!(
                    "{} 的瓦片地址与 {} 协议不匹配，该协议需要 {} 占位符",
                    label,
                    protocol,
                    protocol_placeholder_hint(&protocol)
                )));
            }
        }
        if !(0..=MAX_ZOOM_LIMIT).contains(&layer.min_zoom)
            || !(0..=MAX_ZOOM_LIMIT).contains(&layer.max_zoom)
        {
            return Err(AppError::BadRequest(format!(
                "{} 的缩放层级必须在 0 与 {} 之间",
                label, MAX_ZOOM_LIMIT
            )));
        }
        if layer.min_zoom > layer.max_zoom {
            return Err(AppError::BadRequest(format!(
                "{} 的最小缩放层级不能大于最大缩放层级",
                label
            )));
        }
        if let Some(tile_size) = layer.tile_size {
            if tile_size != 256 && tile_size != 512 {
                return Err(AppError::BadRequest(format!(
                    "{} 的瓦片尺寸必须是 256 或 512",
                    label
                )));
            }
        }
    }
    Ok(())
}

/// Validate the WFS settings. The WFS service is optional, but when a url is
/// configured the SRS must be present as well.
pub fn validate_wfs(wfs: &WfsSettings) -> Result<()> {
    if wfs.url.trim().is_empty() {
        return Ok(());
    }
    if wfs.srs.trim().is_empty() {
        return Err(AppError::BadRequest("WFS 的 SRS 不能为空".to_string()));
    }
    Ok(())
}

/// Normalize a layer list in place: canonicalize every SRS, resolve the tile
/// protocol (auto-detected from the url when not given explicitly), fill in
/// the missing layer ids and the image format.
pub fn normalize_layers(layers: &mut [MapLayer]) {
    for (index, layer) in layers.iter_mut().enumerate() {
        layer.srs = normalize_srs(&layer.srs);
        layer.protocol = layer.protocol.trim().to_ascii_lowercase();
        if layer.protocol.is_empty() {
            layer.protocol = detect_protocol(&layer.url).to_string();
        }
        if layer.id.trim().is_empty() {
            layer.id = format!("layer-{}", index);
        }
        let format = layer
            .format
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if format.is_empty() {
            layer.format = detect_format(&layer.url);
        } else {
            layer.format = Some(format);
        }
        if layer.tile_size.is_none() {
            layer.tile_size = Some(DEFAULT_TILE_SIZE);
        }
        let bing_key = layer.bing_key.as_deref().unwrap_or("").trim();
        layer.bing_key = if bing_key.is_empty() {
            None
        } else {
            Some(bing_key.to_string())
        };
        let imagery_set = layer.imagery_set.as_deref().unwrap_or("").trim();
        layer.imagery_set = if imagery_set.is_empty() {
            Some(DEFAULT_IMAGERY_SET.to_string())
        } else {
            Some(imagery_set.to_string())
        };
    }
}
