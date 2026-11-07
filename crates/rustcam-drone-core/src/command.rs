use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SUPPORTED_COMMAND_KINDS: [&str; 6] =
    ["rtl", "takeoff", "land", "goto", "gimbal", "camera"];

/// 指令状态机取值（service 与 adapter 共用，避免两侧字符串不一致）。
pub mod status {
    /// service 已受理，尚未下发到 adapter。
    pub const PENDING: &str = "pending";
    /// service 已下发到 adapter。
    pub const SENT: &str = "sent";
    /// 飞控返回 MAV_RESULT_IN_PROGRESS，等待最终回执。
    pub const IN_PROGRESS: &str = "in_progress";
    /// 飞控接受 / 无 ACK 指令已下发。
    pub const ACCEPTED: &str = "accepted";
    /// 飞控拒绝或执行失败。
    pub const FAILED: &str = "failed";
    /// 等待 ACK 超时。
    pub const TIMEOUT: &str = "timeout";
    /// service 侧拒绝（未知机型、离线、参数非法等）。
    pub const REJECTED: &str = "rejected";
}

/// 终态：不再变化，超时回收器与重复回执都以此为准。
pub fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        status::ACCEPTED | status::FAILED | status::TIMEOUT | status::REJECTED
    )
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub cmd_id: String,
    pub drone_tag: String,
    pub kind: String,
    pub params: BTreeMap<String, String>,
    pub deadline_ms: i64,
    pub operator: String,
}

impl Command {
    pub fn is_supported_kind(kind: &str) -> bool {
        SUPPORTED_COMMAND_KINDS.contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_kinds_are_supported() {
        assert!(Command::is_supported_kind("rtl"));
        assert!(Command::is_supported_kind("takeoff"));
        assert!(Command::is_supported_kind("camera"));
    }

    #[test]
    fn unknown_kind_is_rejected() {
        assert!(!Command::is_supported_kind("self_destruct"));
    }

    #[test]
    fn terminal_statuses_are_closed() {
        for s in [
            status::ACCEPTED,
            status::FAILED,
            status::TIMEOUT,
            status::REJECTED,
        ] {
            assert!(is_terminal_status(s), "{s} must be terminal");
        }
        for s in [status::PENDING, status::SENT, status::IN_PROGRESS] {
            assert!(!is_terminal_status(s), "{s} must not be terminal");
        }
    }
}
