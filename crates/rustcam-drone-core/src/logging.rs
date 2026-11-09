//! 统一的日志初始化：控制台 + 按小时滚动的日志文件。
//!
//! drone-service 与 drone-adapter 都用同一套，避免各写一份后行为不一致。
//! 日志目录与级别可通过配置或环境变量覆盖（`DRONE_LOG_DIR` / `DRONE_LOG_LEVEL`）。

use std::sync::Once;

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

static INIT: Once = Once::new();

/// 日志配置。
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// 日志目录（默认 `./logs`）。
    pub dir: String,
    /// 日志级别（默认 `info`）。
    pub level: String,
    /// 日志文件名（默认 `drone-service`）。
    pub file_name: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            dir: default_log_dir(),
            level: default_log_level(),
            file_name: "drone-service".into(),
        }
    }
}

fn default_log_dir() -> String {
    "./logs".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl LogConfig {
    pub fn with_file_name(mut self, file_name: impl Into<String>) -> Self {
        self.file_name = file_name.into();
        self
    }

    /// 环境变量覆盖（优先级高于配置文件）。
    pub fn apply_env(&mut self) {
        if let Ok(v) = std::env::var("DRONE_LOG_DIR") {
            if !v.trim().is_empty() {
                self.dir = v;
            }
        }
        if let Ok(v) = std::env::var("DRONE_LOG_LEVEL") {
            if !v.trim().is_empty() {
                self.level = v;
            }
        }
    }
}

/// 初始化日志订阅器（幂等，重复调用只生效一次）。
///
/// 文件按小时滚动（`drone-service.2026-11-08-11`），控制台输出保留，
/// 方便前台直接看日志。
pub fn init(config: &LogConfig) {
    INIT.call_once(|| {
        let _ = std::fs::create_dir_all(&config.dir);

        let file_appender = RollingFileAppender::new(
            Rotation::HOURLY,
            &config.dir,
            &config.file_name,
        );
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(config.level.clone()));

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(std::io::stdout),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .with_file(true)
                    .with_line_number(true)
                    .with_writer(non_blocking),
            );

        let _ = subscriber.try_init();
        // guard 必须存活到进程结束，否则文件写入会被丢弃。
        std::mem::forget(guard);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_to_logs_dir() {
        let cfg = LogConfig::default();
        assert_eq!(cfg.dir, "./logs");
        assert_eq!(cfg.level, "info");
    }

    #[test]
    fn file_name_can_be_overridden() {
        let cfg = LogConfig::default().with_file_name("drone-adapter");
        assert_eq!(cfg.file_name, "drone-adapter");
    }
}
