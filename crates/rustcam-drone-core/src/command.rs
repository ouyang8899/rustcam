use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SUPPORTED_COMMAND_KINDS: [&str; 6] =
    ["rtl", "takeoff", "land", "goto", "gimbal", "camera"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub cmd_id: String,
    pub device_id: String,
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
    }

    #[test]
    fn unknown_kind_is_rejected() {
        assert!(!Command::is_supported_kind("self_destruct"));
    }
}
