use serde::Deserialize;

fn default_grpc_addr() -> String {
    "0.0.0.0:50051".into()
}
fn default_metrics_addr() -> String {
    "0.0.0.0:50052".into()
}
fn default_heartbeat() -> u64 {
    10
}
fn default_lease() -> u32 {
    30
}
fn default_max_inflight() -> usize {
    1024
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".into()
}
fn default_database_url() -> String {
    "postgres://postgres:123456@127.0.0.1:5435/rustcam".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct DroneConfig {
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,
    #[serde(default = "default_heartbeat")]
    pub adapter_heartbeat_sec: u64,
    #[serde(default = "default_lease")]
    pub default_lease_sec: u32,
    #[serde(default)]
    pub risk_enabled: bool,
    #[serde(default = "default_max_inflight")]
    pub max_inflight_commands: usize,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    #[serde(default = "default_database_url")]
    pub database_url: String,
}

impl Default for DroneConfig {
    fn default() -> Self {
        Self {
            grpc_addr: default_grpc_addr(),
            metrics_addr: default_metrics_addr(),
            adapter_heartbeat_sec: default_heartbeat(),
            default_lease_sec: default_lease(),
            risk_enabled: false,
            max_inflight_commands: default_max_inflight(),
            redis_url: default_redis_url(),
            database_url: default_database_url(),
        }
    }
}

impl DroneConfig {
    /// 解析 TOML 文本；缺失字段使用默认值。
    pub fn from_toml_str(text: &str) -> DroneConfig {
        let settings = config::Config::builder()
            .add_source(config::File::from_str(text, config::FileFormat::Toml))
            .build();
        match settings.and_then(|s| s.try_deserialize::<DroneConfig>()) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(error = %e, "[DroneConfig] parse failed, falling back to defaults");
                DroneConfig::default()
            }
        }
    }

    /// 环境变量覆盖（优先级高于文件）。
    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("DRONE_GRPC_ADDR") {
            self.grpc_addr = v;
        }
        if let Ok(v) = std::env::var("DRONE_METRICS_ADDR") {
            self.metrics_addr = v;
        }
        if let Ok(v) = std::env::var("DRONE_REDIS_URL") {
            self.redis_url = v;
        }
        if let Ok(v) = std::env::var("DRONE_DATABASE_URL") {
            self.database_url = v;
        }
        if let Ok(v) = std::env::var("DRONE_RISK_ENABLED") {
            self.risk_enabled = matches!(v.as_str(), "1" | "true" | "yes");
        }
    }

    /// 读工作目录下的 drone.toml（不存在则用默认值）。
    pub fn load() -> anyhow::Result<DroneConfig> {
        let mut cfg = match std::fs::read_to_string("drone.toml") {
            Ok(text) => DroneConfig::from_toml_str(&text),
            Err(e) => {
                tracing::warn!(error = %e, "[DroneConfig] drone.toml not found, using defaults");
                DroneConfig::default()
            }
        };
        cfg.apply_env();
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_applied_when_fields_missing() {
        let cfg = DroneConfig::from_toml_str("");
        assert_eq!(cfg.grpc_addr, "0.0.0.0:50051");
        assert_eq!(cfg.metrics_addr, "0.0.0.0:50052");
        assert_eq!(cfg.adapter_heartbeat_sec, 10);
        assert_eq!(cfg.default_lease_sec, 30);
        assert!(!cfg.risk_enabled);
        assert_eq!(cfg.max_inflight_commands, 1024);
    }

    #[test]
    fn explicit_values_win() {
        let cfg = DroneConfig::from_toml_str(
            "grpc_addr = \"0.0.0.0:60051\"\nmetrics_addr = \"0.0.0.0:60052\"\nredis_url = \"redis://x:6379\"\n",
        );
        assert_eq!(cfg.grpc_addr, "0.0.0.0:60051");
        assert_eq!(cfg.metrics_addr, "0.0.0.0:60052");
        assert_eq!(cfg.redis_url, "redis://x:6379");
    }

    #[test]
    fn env_overrides_file_values() {
        let mut cfg = DroneConfig::from_toml_str("grpc_addr = \"0.0.0.0:60051\"\n");
        std::env::set_var("DRONE_GRPC_ADDR", "0.0.0.0:70051");
        cfg.apply_env();
        std::env::remove_var("DRONE_GRPC_ADDR");
        assert_eq!(cfg.grpc_addr, "0.0.0.0:70051");
    }
}
