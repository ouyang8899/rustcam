use serde::Deserialize;

fn default_adapter_id() -> String { "adapter-mavlink-1".into() }
fn default_protocol() -> String { "mavlink".into() }
fn default_service_addr() -> String { "http://127.0.0.1:50051".into() }
fn default_mavlink_addr() -> String { "udp://0.0.0.0:14540".into() }
fn default_reconnect_min_secs() -> u64 { 1 }
fn default_reconnect_max_secs() -> u64 { 30 }
fn default_telemetry_hz() -> u32 { 10 }
fn default_buffer_size() -> usize { 10000 }

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    #[serde(default = "default_adapter_id")]
    pub adapter_id: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_service_addr")]
    pub service_addr: String,
    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default = "default_mavlink_addr")]
    pub mavlink_addr: String,
    #[serde(default = "default_reconnect_min_secs")]
    pub reconnect_min_secs: u64,
    #[serde(default = "default_reconnect_max_secs")]
    pub reconnect_max_secs: u64,
    #[serde(default = "default_telemetry_hz")]
    pub telemetry_hz: u32,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            adapter_id: default_adapter_id(),
            protocol: default_protocol(),
            service_addr: default_service_addr(),
            devices: Vec::new(),
            mavlink_addr: default_mavlink_addr(),
            reconnect_min_secs: default_reconnect_min_secs(),
            reconnect_max_secs: default_reconnect_max_secs(),
            telemetry_hz: default_telemetry_hz(),
            buffer_size: default_buffer_size(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AdapterFile {
    #[serde(default)]
    adapter: Option<AdapterConfig>,
}

impl AdapterConfig {
    pub fn from_toml_str(text: &str) -> AdapterConfig {
        let settings = config::Config::builder()
            .add_source(config::File::from_str(text, config::FileFormat::Toml))
            .build();
        let settings = match settings {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "[AdapterConfig] parse failed, using defaults");
                return AdapterConfig::default();
            }
        };

        // drone.md 使用 [adapter] 表；同时兼容扁平写法（测试与旧配置）。
        let nested = settings
            .clone()
            .try_deserialize::<AdapterFile>()
            .ok()
            .and_then(|f| f.adapter);
        match nested {
            Some(cfg) => cfg,
            None => settings.try_deserialize::<AdapterConfig>().unwrap_or_else(|e| {
                tracing::warn!(error = %e, "[AdapterConfig] parse failed, using defaults");
                AdapterConfig::default()
            }),
        }
    }

    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("ADAPTER_ID") { self.adapter_id = v; }
        if let Ok(v) = std::env::var("PROTOCOL") { self.protocol = v; }
        if let Ok(v) = std::env::var("SERVICE_ADDR") { self.service_addr = v; }
        if let Ok(v) = std::env::var("MAVLINK_ADDR") { self.mavlink_addr = v; }
        if let Ok(v) = std::env::var("DEVICES") {
            self.devices = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
    }

    pub fn load() -> anyhow::Result<AdapterConfig> {
        let mut cfg = match std::fs::read_to_string("adapter.toml") {
            Ok(text) => AdapterConfig::from_toml_str(&text),
            Err(_) => AdapterConfig::default(),
        };
        cfg.apply_env();
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_used_when_missing() {
        let cfg = AdapterConfig::from_toml_str("");
        assert_eq!(cfg.adapter_id, "adapter-mavlink-1");
        assert_eq!(cfg.protocol, "mavlink");
        assert_eq!(cfg.service_addr, "http://127.0.0.1:50051");
        assert!(cfg.devices.is_empty());
    }

    #[test]
    fn devices_are_parsed_from_toml() {
        let cfg = AdapterConfig::from_toml_str("devices = [\"drone-001\", \"drone-002\"]\n");
        assert_eq!(cfg.devices, vec!["drone-001".to_string(), "drone-002".to_string()]);
    }

    #[test]
    fn runtime_defaults_match_drone_design() {
        let cfg = AdapterConfig::default();
        assert_eq!(cfg.reconnect_min_secs, 1);
        assert_eq!(cfg.reconnect_max_secs, 30);
        assert_eq!(cfg.telemetry_hz, 10);
        assert_eq!(cfg.buffer_size, 10000);
    }

    #[test]
    fn nested_adapter_table_is_parsed() {
        let cfg = AdapterConfig::from_toml_str(
            "[adapter]\nadapter_id = \"adapter-dji-1\"\nprotocol = \"dji\"\nservice_addr = \"http://10.0.0.5:50051\"\ndevices = [\"drone-009\"]\n",
        );
        assert_eq!(cfg.adapter_id, "adapter-dji-1");
        assert_eq!(cfg.protocol, "dji");
        assert_eq!(cfg.service_addr, "http://10.0.0.5:50051");
        assert_eq!(cfg.devices, vec!["drone-009".to_string()]);
    }
}
