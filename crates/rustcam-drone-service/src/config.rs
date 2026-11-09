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
fn default_command_timeout_sec() -> u32 {
    30
}
fn default_require_control() -> bool {
    true
}
fn default_route_default_end_action() -> String {
    "hold".into()
}
fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".into()
}
fn default_database_url() -> String {
    "postgres://postgres:123456@127.0.0.1:5435/rustcam".into()
}
fn default_log_dir() -> String {
    "./logs".into()
}
fn default_log_level() -> String {
    "info".into()
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
    /// 超速阈值（米/秒）；0 表示使用默认值 30。
    #[serde(default)]
    pub risk_max_speed_mps: f64,
    #[serde(default = "default_max_inflight")]
    pub max_inflight_commands: usize,
    /// 指令未显式给 deadline 时使用的默认超时。
    #[serde(default = "default_command_timeout_sec")]
    pub default_command_timeout_sec: u32,
    /// 飞行指令是否必须先持有控制权租约（产品默认开启）。
    #[serde(default = "default_require_control")]
    pub require_control: bool,
    /// 航线执行完成后是否由服务端自动返航（RTL）。
    #[serde(default = "default_route_default_end_action")]
    pub route_default_end_action: String,
    /// 状态快照文件；设置后重启可恢复档案/航线/围栏/审计。为空表示不持久化。
    #[serde(default)]
    pub state_file: Option<String>,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    /// 签名密钥的主密钥（64 位十六进制 = 32 字节）。
    ///
    /// 未配置时不下发签名密钥（该机按"不需要签名"处理），并记录告警。
    #[serde(default)]
    pub signing_master_key: Option<String>,
    /// 日志目录（默认 `./logs`）。
    #[serde(default = "default_log_dir")]
    pub log_dir: String,
    /// 日志级别（默认 `info`）。
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl Default for DroneConfig {
    fn default() -> Self {
        Self {
            grpc_addr: default_grpc_addr(),
            metrics_addr: default_metrics_addr(),
            adapter_heartbeat_sec: default_heartbeat(),
            default_lease_sec: default_lease(),
            risk_enabled: false,
            risk_max_speed_mps: 0.0,
            max_inflight_commands: default_max_inflight(),
            default_command_timeout_sec: default_command_timeout_sec(),
            require_control: default_require_control(),
            route_default_end_action: default_route_default_end_action(),
            state_file: None,
            redis_url: default_redis_url(),
            database_url: default_database_url(),
            signing_master_key: None,
            log_dir: default_log_dir(),
            log_level: default_log_level(),
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
        if let Ok(v) = std::env::var("DRONE_COMMAND_TIMEOUT_SEC") {
            if let Ok(sec) = v.parse() {
                self.default_command_timeout_sec = sec;
            }
        }
        if let Ok(v) = std::env::var("DRONE_SIGNING_MASTER_KEY") {
            let v = v.trim().to_string();
            self.signing_master_key = if v.is_empty() { None } else { Some(v) };
        }
        if let Ok(v) = std::env::var("DRONE_LOG_DIR") {
            if !v.trim().is_empty() {
                self.log_dir = v;
            }
        }
        if let Ok(v) = std::env::var("DRONE_LOG_LEVEL") {
            if !v.trim().is_empty() {
                self.log_level = v;
            }
        }
    }

    /// 校验配置；主密钥格式错误时直接拒绝启动。
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(key) = &self.signing_master_key {
            crate::secret::SecretBox::from_hex(key)
                .map_err(|e| anyhow::anyhow!("DRONE_SIGNING_MASTER_KEY invalid: {e}"))?;
        }
        Ok(())
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
        cfg.validate()?;
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
        assert_eq!(cfg.default_command_timeout_sec, 30);
    }

    #[test]
    fn log_settings_default_to_logs_dir() {
        let cfg = DroneConfig::from_toml_str("");
        assert_eq!(cfg.log_dir, "./logs");
        assert_eq!(cfg.log_level, "info");
    }

    #[test]
    fn log_settings_can_be_overridden() {
        let cfg = DroneConfig::from_toml_str(
            "log_dir = \"/var/log/rustcam\"\nlog_level = \"debug\"\n",
        );
        assert_eq!(cfg.log_dir, "/var/log/rustcam");
        assert_eq!(cfg.log_level, "debug");
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
