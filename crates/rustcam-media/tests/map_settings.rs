use rustcam_media::domain::{
    detect_format, detect_protocol, normalize_layers, normalize_srs, protocol_flips_y,
    protocol_matches_url, validate_layers, validate_wfs, LayerDim, MapLayer, WfsSettings,
    PROTOCOL_QUADKEY, PROTOCOL_TMS, PROTOCOL_WMTS, PROTOCOL_XYZ,
};

fn layer(srs: &str) -> MapLayer {
    MapLayer {
        id: String::new(),
        name: "test".into(),
        dim: LayerDim::TwoD,
        url: "https://host/{z}/{x}/{y}.png".into(),
        srs: srs.into(),
        protocol: String::new(),
        format: None,
        tile_size: None,
        bing_key: None,
        imagery_set: None,
        min_zoom: 0,
        max_zoom: 19,
        enabled: true,
    }
}

#[test]
fn normalize_srs_canonicalizes_epsg_prefix() {
    assert_eq!(normalize_srs("epsg:3857"), "EPSG:3857");
    assert_eq!(normalize_srs("  EPSG:4326  "), "EPSG:4326");
    assert_eq!(
        normalize_srs("+proj=longlat +datum=WGS84"),
        "+proj=longlat +datum=WGS84"
    );
    assert_eq!(normalize_srs(""), "");
}

#[test]
fn validate_layers_rejects_empty_url() {
    let mut l = layer("EPSG:3857");
    l.url = "   ".into();
    assert!(validate_layers(&[l]).is_err());
}

#[test]
fn validate_layers_rejects_empty_srs() {
    let mut l = layer("EPSG:3857");
    l.srs = "  ".into();
    assert!(validate_layers(&[l]).is_err());
}

#[test]
fn validate_layers_rejects_zoom_out_of_range() {
    let mut l = layer("EPSG:3857");
    l.max_zoom = 25;
    assert!(validate_layers(&[l]).is_err());

    let mut l2 = layer("EPSG:3857");
    l2.min_zoom = -1;
    assert!(validate_layers(&[l2]).is_err());
}

#[test]
fn validate_layers_rejects_min_above_max() {
    let mut l = layer("EPSG:3857");
    l.min_zoom = 12;
    l.max_zoom = 5;
    assert!(validate_layers(&[l]).is_err());
}

#[test]
fn validate_layers_accepts_valid_set() {
    assert!(validate_layers(&[layer("EPSG:3857"), layer("EPSG:4326")]).is_ok());
    assert!(validate_layers(&[]).is_ok());
}

#[test]
fn validate_wfs_allows_empty_url() {
    assert!(validate_wfs(&WfsSettings::default()).is_ok());
}

#[test]
fn validate_wfs_requires_srs_when_url_present() {
    let wfs = WfsSettings {
        url: "http://host/ows".into(),
        srs: String::new(),
    };
    assert!(validate_wfs(&wfs).is_err());
}

#[test]
fn validate_wfs_accepts_url_with_srs() {
    let wfs = WfsSettings {
        url: "http://host/ows".into(),
        srs: "EPSG:4326".into(),
    };
    assert!(validate_wfs(&wfs).is_ok());
}

#[test]
fn protocol_is_detected_from_the_url_structure() {
    assert_eq!(detect_protocol("https://h/{z}/{x}/{y}.png"), PROTOCOL_XYZ);
    assert_eq!(detect_protocol("https://h/{z}/{x}/{-y}.png"), PROTOCOL_TMS);
    assert_eq!(
        detect_protocol("https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1"),
        PROTOCOL_QUADKEY
    );
    assert_eq!(
        detect_protocol(
            "https://t0.tianditu.gov.cn/img_wmts?SERVICE=WMTS&TILEMATRIX={z}&TILEROW={y}&TILECOL={x}"
        ),
        PROTOCOL_WMTS
    );
}

#[test]
fn protocol_matching_and_flip_rules() {
    assert!(protocol_matches_url(
        "https://h/{z}/{x}/{y}.png",
        PROTOCOL_XYZ
    ));
    assert!(protocol_matches_url(
        "https://h/{z}/{x}/{y}.png",
        PROTOCOL_TMS
    ));
    assert!(!protocol_matches_url(
        "https://h/{z}/{x}/{y}.png",
        PROTOCOL_QUADKEY
    ));
    assert!(protocol_matches_url(
        "https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1",
        PROTOCOL_QUADKEY
    ));
    assert!(protocol_flips_y(PROTOCOL_TMS));
    assert!(!protocol_flips_y(PROTOCOL_XYZ));
    assert!(!protocol_flips_y(PROTOCOL_WMTS));
}

#[test]
fn validate_layers_rejects_unknown_protocol() {
    let mut l = layer("EPSG:3857");
    l.protocol = "wmtz".into();
    assert!(validate_layers(&[l]).is_err());
}

#[test]
fn validate_layers_requires_protocol_matching_template() {
    // 声明 quadkey 却给 xyz 模板
    let mut l = layer("EPSG:3857");
    l.protocol = PROTOCOL_QUADKEY.into();
    assert!(validate_layers(&[l]).is_err());

    // 声明 wmts 却给普通 xyz 模板
    let mut l2 = layer("EPSG:3857");
    l2.protocol = PROTOCOL_WMTS.into();
    assert!(validate_layers(&[l2]).is_err());

    // 匹配时通过；protocol 留空则交给自动推断
    let mut l3 = layer("EPSG:3857");
    l3.protocol = PROTOCOL_XYZ.into();
    assert!(validate_layers(&[l3]).is_ok());
    assert!(validate_layers(&[layer("EPSG:3857")]).is_ok());
}

#[test]
fn normalize_layers_resolves_protocol_and_format() {
    let mut l = layer("epsg:3857");
    l.url = "https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1".into();
    l.format = None;
    let mut layers = vec![l];
    normalize_layers(&mut layers);
    assert_eq!(layers[0].srs, "EPSG:3857");
    assert_eq!(layers[0].protocol, PROTOCOL_QUADKEY);
    assert_eq!(layers[0].format.as_deref(), Some("jpeg"));
    assert_eq!(layers[0].id, "layer-0");

    let mut l2 = layer("EPSG:3857");
    l2.format = None;
    let mut layers2 = vec![l2];
    normalize_layers(&mut layers2);
    assert_eq!(layers2[0].protocol, PROTOCOL_XYZ);
    assert_eq!(layers2[0].format.as_deref(), Some("png"));

    assert_eq!(detect_format("https://h/a.jpg"), Some("jpeg".to_string()));
    assert_eq!(
        detect_format("https://h/a.png?x=1"),
        Some("png".to_string())
    );
    assert_eq!(detect_format("https://h/{z}/{x}/{y}"), None);
}

#[test]
fn validate_layers_rejects_invalid_tile_size() {
    let mut l = layer("EPSG:3857");
    l.tile_size = Some(1024);
    assert!(validate_layers(&[l]).is_err());

    let mut l2 = layer("EPSG:3857");
    l2.tile_size = Some(512);
    assert!(validate_layers(&[l2]).is_ok());
}

#[test]
fn normalize_layers_defaults_tile_size() {
    let mut layers = vec![layer("EPSG:3857")];
    normalize_layers(&mut layers);
    assert_eq!(layers[0].tile_size, Some(256));
    assert_eq!(layers[0].imagery_set.as_deref(), Some("Aerial"));
    assert_eq!(layers[0].bing_key, None);
}

#[test]
fn normalize_layers_keeps_bing_key() {
    let mut l = layer("EPSG:3857");
    l.bing_key = Some("  KEY123  ".into());
    let mut layers = vec![l];
    normalize_layers(&mut layers);
    assert_eq!(layers[0].bing_key.as_deref(), Some("KEY123"));
}

#[test]
fn map_settings_exposes_no_end_level_zoom() {
    let settings = rustcam_media::domain::map_settings::MapSettings::default();
    let json = serde_json::to_value(settings).unwrap();
    let object = json.as_object().unwrap();
    for key in [
        "browser_min_zoom",
        "browser_max_zoom",
        "client_min_zoom",
        "client_max_zoom",
    ] {
        assert!(
            !object.contains_key(key),
            "{key} must be derived from the layers, not configured per end"
        );
    }
}

#[test]
fn layer_dim_serializes_as_2d_and_3d() {
    assert_eq!(serde_json::to_string(&LayerDim::TwoD).unwrap(), "\"2d\"");
    assert_eq!(serde_json::to_string(&LayerDim::ThreeD).unwrap(), "\"3d\"");
    let parsed: LayerDim = serde_json::from_str("\"3d\"").unwrap();
    assert_eq!(parsed, LayerDim::ThreeD);
    assert!(serde_json::from_str::<LayerDim>("\"4d\"").is_err());
}
