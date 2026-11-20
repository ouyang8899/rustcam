//! 查询过滤值的归一化。
//!
//! 数据库列里设备状态存的是小写（`status_to_str`）、协议存的是大写形式
//! （`protocol_to_str`），而 API 对外暴露的是 PascalCase。前端按 PascalCase 发送时
//! 直接拼进 SQL 会永远匹配不到行（`AND status = 'Online'` 匹配不到存着 `online` 的行），
//! 因此统一在进入 SQL 之前归一化。
//!
//! 无法识别的取值返回 `None`，含义是"不按该维度过滤"，而不是"过滤出空集"。

fn canonical(v: Option<&str>, table: &[(&str, &str)]) -> Option<String> {
    let raw = v?.trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_lowercase();
    table
        .iter()
        .find(|(key, _)| *key == lower)
        .map(|(_, value)| value.to_string())
}

const STATUS_TABLE: &[(&str, &str)] = &[
    ("offline", "offline"),
    ("online", "online"),
    ("error", "error"),
    ("maintaining", "maintaining"),
];

const PROTOCOL_TABLE: &[(&str, &str)] = &[
    ("gb28181", "GB28181"),
    ("onvif", "ONVIF"),
    ("rtsp", "RTSP"),
    ("rtmp", "RTMP"),
    ("hls", "HLS"),
    ("webrtc", "WebRTC"),
];

const DEVICE_TYPE_TABLE: &[(&str, &str)] = &[
    ("nvr", "NVR"),
    ("ipc", "IPC"),
    ("dvr", "DVR"),
    ("encoder", "Encoder"),
    ("videoserver", "VideoServer"),
    ("camera", "Camera"),
    ("platform", "Platform"),
    ("other", "Other"),
];

pub fn normalize_status(v: Option<&str>) -> Option<String> {
    canonical(v, STATUS_TABLE)
}

pub fn normalize_protocol(v: Option<&str>) -> Option<String> {
    canonical(v, PROTOCOL_TABLE)
}

pub fn normalize_device_type(v: Option<&str>) -> Option<String> {
    canonical(v, DEVICE_TYPE_TABLE)
}
