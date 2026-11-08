use serde::Deserialize;

fn default_adapter_id() -> String {
    "adapter-mavlink-1".into()
}
fn default_protocol() -> String {
    "mavlink".into()
}
fn default_service_addr() -> String {
    "http://127.0.0.1:50051".into()
}
fn default_autopilot() -> String {
    "auto".into()
}
fn default_reconnect_min_secs() -> u64 {
    1
}
fn default_reconnect_max_secs() -> u64 {
    30
}
fn default_telemetry_hz() -> u32 {
    10
}
fn default_buffer_size() -> usize {
    10000
}
fn default_heartbeat_interval_secs() -> u64 {
    10
}
fn default_heartbeat_timeout_ms() -> u64 {
    5000
}
fn default_command_timeout_ms() -> u64 {
    10000
}
fn default_source_system() -> u8 {
    255
}
fn default_source_component() -> u8 {
    190
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    #[serde(default = "default_adapter_id")]
    pub adapter_id: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_service_addr")]
    pub service_addr: String,
    /// 飞控类型提示：`auto`（按 HEARTBEAT 自动识别）/ `px4` / `ardupilot`。
    #[serde(default = "default_autopilot")]
    pub autopilot: String,
    #[serde(default = "default_source_system")]
    pub source_system: u8,
    #[serde(default = "default_source_component")]
    pub source_component: u8,
    #[serde(default = "default_reconnect_min_secs")]
    pub reconnect_min_secs: u64,
    #[serde(default = "default_reconnect_max_secs")]
    pub reconnect_max_secs: u64,
    #[serde(default = "default_telemetry_hz")]
    pub telemetry_hz: u32,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    /// adapter → service 心跳间隔。
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
    /// 超过该时间收不到飞控 HEARTBEAT 视为链路丢失。
    #[serde(default = "default_heartbeat_timeout_ms")]
    pub heartbeat_timeout_ms: u64,
    /// 指令等待 COMMAND_ACK 的默认超时。
    #[serde(default = "default_command_timeout_ms")]
    pub command_timeout_ms: u64,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            adapter_id: default_adapter_id(),
            protocol: default_protocol(),
            service_addr: default_service_addr(),
            autopilot: default_autopilot(),
            source_system: default_source_system(),
            source_component: default_source_component(),
            reconnect_min_secs: default_reconnect_min_secs(),
            reconnect_max_secs: default_reconnect_max_secs(),
            telemetry_hz: default_telemetry_hz(),
            buffer_size: default_buffer_size(),
            heartbeat_interval_secs: default_heartbeat_interval_secs(),
            heartbeat_timeout_ms: default_heartbeat_timeout_ms(),
            command_timeout_ms: default_command_timeout_ms(),
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

        // adapter.toml 使用 [adapter] 表；同时兼容扁平写法（测试与旧配置）。
        let nested = settings
            .clone()
            .try_deserialize::<AdapterFile>()
            .ok()
            .and_then(|f| f.adapter);
        match nested {
            Some(cfg) => cfg,
            None => settings
                .try_deserialize::<AdapterConfig>()
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "[AdapterConfig] parse failed, using defaults");
                    AdapterConfig::default()
                }),
        }
    }

    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("ADAPTER_ID") {
            self.adapter_id = v;
        }
        if let Ok(v) = std::env::var("PROTOCOL") {
            self.protocol = v;
        }
        if let Ok(v) = std::env::var("SERVICE_ADDR") {
            self.service_addr = v;
        }
        if let Ok(v) = std::env::var("MAVLINK_AUTOPILOT") {
            self.autopilot = v;
        }
        if let Ok(v) = std::env::var("TELEMETRY_HZ") {
            if let Ok(hz) = v.parse() {
                self.telemetry_hz = hz;
            }
        }
    }

    pub fn telemetry_period(&self) -> std::time::Duration {
        let hz = self.telemetry_hz.clamp(1, 50);
        std::time::Duration::from_millis((1000 / hz as u64).max(1))
    }

    /// 校验关键字段；非法配置直接拒绝启动。
    ///
    /// 注意：MAVLink 端点与设备标识**不在本地配置**——由平台按 `adapter_id`
    /// 分配下发，因此这里只校验 adapter 自身的参数。
    pub fn validate(&self) -> anyhow::Result<()> {
        let autopilot = self.autopilot.trim().to_ascii_lowercase();
        if !matches!(
            autopilot.as_str(),
            "auto" | "px4" | "ardupilot" | "ardupilotmega" | "apm"
        ) {
            anyhow::bail!(
                "unsupported autopilot `{}` (expected auto|px4|ardupilot)",
                self.autopilot
            );
        }
        if self.service_addr.trim().is_empty() {
            anyhow::bail!("service_addr must not be empty");
        }
        if self.heartbeat_timeout_ms < 1000 {
            anyhow::bail!("heartbeat_timeout_ms must be >= 1000");
        }
        Ok(())
    }

    pub fn load() -> anyhow::Result<AdapterConfig> {
        let mut cfg = match std::fs::read_to_string("adapter.toml") {
            Ok(text) => AdapterConfig::from_toml_str(&text),
            Err(_) => AdapterConfig::default(),
        };
        cfg.apply_env();
        cfg.validate()?;
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
        assert_eq!(cfg.autopilot, "auto");
        assert_eq!(cfg.source_system, 255);
        assert_eq!(cfg.source_component, 190);
        assert_eq!(cfg.heartbeat_timeout_ms, 5000);
    }

    #[test]
    fn endpoint_is_not_mandatory_locally() {
        // 端点改由服务端分配，本地不再必填。
        let cfg = AdapterConfig::default();
        cfg.validate().expect("只带 adapter_id 与 service_addr 应当合法");
    }

    #[test]
    fn runtime_defaults_match_drone_design() {
        let cfg = AdapterConfig::default();
        assert_eq!(cfg.reconnect_min_secs, 1);
        assert_eq!(cfg.reconnect_max_secs, 30);
        assert_eq!(cfg.telemetry_hz, 10);
        assert_eq!(cfg.buffer_size, 10000);
        assert_eq!(
            cfg.telemetry_period(),
            std::time::Duration::from_millis(100)
        );
    }

    #[test]
    fn nested_adapter_table_is_parsed() {
        let cfg = AdapterConfig::from_toml_str(
            "[adapter]\nadapter_id = \"adapter-px4-1\"\nprotocol = \"mavlink\"\n\
             service_addr = \"http://10.0.0.5:50051\"\nautopilot = \"px4\"\n",
        );
        assert_eq!(cfg.adapter_id, "adapter-px4-1");
        assert_eq!(cfg.service_addr, "http://10.0.0.5:50051");
        assert_eq!(cfg.autopilot, "px4");
    }

    #[test]
    fn invalid_autopilot_is_rejected() {
        let cfg = AdapterConfig {
            autopilot: "dji".into(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
        let cfg = AdapterConfig {
            autopilot: "ArduPilot".into(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
        let cfg = AdapterConfig {
            autopilot: "APM".into(),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn telemetry_period_is_clamped() {
        let cfg = AdapterConfig {
            telemetry_hz: 0,
            ..Default::default()
        };
        assert_eq!(
            cfg.telemetry_period(),
            std::time::Duration::from_millis(1000)
        );
        let cfg = AdapterConfig {
            telemetry_hz: 1000,
            ..Default::default()
        };
        assert_eq!(cfg.telemetry_period(), std::time::Duration::from_millis(20));
    }
}
