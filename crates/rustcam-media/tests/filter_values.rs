use rustcam_media::common::{normalize_device_type, normalize_protocol, normalize_status};

#[test]
fn status_is_case_insensitive() {
    assert_eq!(normalize_status(Some("Online")).as_deref(), Some("online"));
    assert_eq!(normalize_status(Some("ONLINE")).as_deref(), Some("online"));
    assert_eq!(
        normalize_status(Some("  Offline ")).as_deref(),
        Some("offline")
    );
    assert_eq!(
        normalize_status(Some("Maintaining")).as_deref(),
        Some("maintaining")
    );
    assert_eq!(normalize_status(Some("Error")).as_deref(), Some("error"));
}

#[test]
fn status_absent_or_unknown_yields_no_filter() {
    assert_eq!(normalize_status(None), None);
    assert_eq!(normalize_status(Some("")), None);
    assert_eq!(normalize_status(Some("   ")), None);
    assert_eq!(normalize_status(Some("bogus")), None);
}

#[test]
fn protocol_is_case_insensitive() {
    assert_eq!(
        normalize_protocol(Some("Gb28181")).as_deref(),
        Some("GB28181")
    );
    assert_eq!(
        normalize_protocol(Some("gb28181")).as_deref(),
        Some("GB28181")
    );
    assert_eq!(normalize_protocol(Some("Rtsp")).as_deref(), Some("RTSP"));
    assert_eq!(normalize_protocol(Some("Onvif")).as_deref(), Some("ONVIF"));
    assert_eq!(normalize_protocol(Some("Rtmp")).as_deref(), Some("RTMP"));
    assert_eq!(
        normalize_protocol(Some("webrtc")).as_deref(),
        Some("WebRTC")
    );
    assert_eq!(normalize_protocol(Some("hls")).as_deref(), Some("HLS"));
}

#[test]
fn device_type_is_case_insensitive() {
    assert_eq!(normalize_device_type(Some("IPC")).as_deref(), Some("IPC"));
    assert_eq!(normalize_device_type(Some("ipc")).as_deref(), Some("IPC"));
    assert_eq!(normalize_device_type(Some("nvr")).as_deref(), Some("NVR"));
    assert_eq!(
        normalize_device_type(Some("videoserver")).as_deref(),
        Some("VideoServer")
    );
    assert_eq!(normalize_device_type(Some("nope")), None);
}
