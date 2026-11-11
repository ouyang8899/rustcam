//! 平台分配的差量计算：哪台机体要建链、哪台要重建、哪台要关链。
//!
//! 平台下发的是**全量快照**，adapter 与"当前生效配置"比较得出差量，
//! 只对真正变化的机体动作——未变化的机体不闪断。

use std::collections::HashMap;

use rustcam_drone_proto::link::DeviceAssignment;

/// 一台机体在两次分配快照之间的动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentAction {
    /// 新增：建链。
    Connect,
    /// 链路参数变化：重建链路。
    Reconnect,
    /// 快照里消失（档案删除或换归属）：关链。
    Disconnect,
    /// 无变化：保持不动。
    Keep,
}

/// 差量结果：机体标识 → 动作。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssignmentDiff {
    pub actions: Vec<(String, AssignmentAction)>,
}

/// 比较两次分配快照，得到每台机体要做的动作。
///
/// 只比较 adapter 真正消费的字段（端点 / sysid / compid）：
/// 签名密钥、协议版本等字段 adapter 并未消费，变化不触发重建。
pub fn diff_assignments(
    applied: &HashMap<String, DeviceAssignment>,
    next: &[DeviceAssignment],
) -> AssignmentDiff {
    let mut actions: Vec<(String, AssignmentAction)> = Vec::new();
    let mut seen: HashMap<&str, &DeviceAssignment> = HashMap::new();
    for assignment in next {
        seen.insert(assignment.drone_tag.as_str(), assignment);
        let action = match applied.get(&assignment.drone_tag) {
            None => AssignmentAction::Connect,
            Some(old) => {
                if link_params_changed(old, assignment) {
                    AssignmentAction::Reconnect
                } else {
                    AssignmentAction::Keep
                }
            }
        };
        actions.push((assignment.drone_tag.clone(), action));
    }
    for tag in applied.keys() {
        if !seen.contains_key(tag.as_str()) {
            actions.push((tag.clone(), AssignmentAction::Disconnect));
        }
    }
    AssignmentDiff { actions }
}

/// adapter 消费的链路参数是否变化。
fn link_params_changed(old: &DeviceAssignment, new: &DeviceAssignment) -> bool {
    old.mavlink_addr != new.mavlink_addr
        || old.mavlink_sysid != new.mavlink_sysid
        || old.mavlink_compid != new.mavlink_compid
}

/// 维护"当前生效配置"，供下一次差量比较。
#[derive(Debug, Clone, Default)]
pub struct AssignmentTracker {
    applied: HashMap<String, DeviceAssignment>,
}

/// 一次差量应用的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// 新建的链路数。
    pub connected: usize,
    /// 重建的链路数。
    pub reconnected: usize,
    /// 关闭的链路数。
    pub disconnected: usize,
    /// 失败次数（失败时旧链保留）。
    pub failed: usize,
}

impl AssignmentTracker {
    /// 用注册时下发的分配初始化。
    pub fn new(initial: &[DeviceAssignment]) -> AssignmentTracker {
        AssignmentTracker {
            applied: initial
                .iter()
                .map(|a| (a.drone_tag.clone(), a.clone()))
                .collect(),
        }
    }

    /// 当前生效配置。
    pub fn applied(&self) -> &HashMap<String, DeviceAssignment> {
        &self.applied
    }

    /// 记录某台机新的生效配置。
    pub fn set_applied(&mut self, drone_tag: &str, assignment: DeviceAssignment) {
        self.applied.insert(drone_tag.to_string(), assignment);
    }

    /// 清除某台机的生效配置（该机已离开本 adapter）。
    pub fn clear_applied(&mut self, drone_tag: &str) {
        self.applied.remove(drone_tag);
    }

    /// 用一整份快照替换生效配置（注册 / 重连时用）。
    pub fn replace_all(&mut self, assignments: &[DeviceAssignment]) {
        self.applied = assignments
            .iter()
            .map(|a| (a.drone_tag.clone(), a.clone()))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustcam_drone_proto::link::DeviceAssignment;

    fn assignment(tag: &str, addr: &str, sysid: i32) -> DeviceAssignment {
        DeviceAssignment {
            drone_tag: tag.into(),
            mavlink_addr: addr.into(),
            mavlink_sysid: sysid,
            mavlink_compid: 1,
            ..Default::default()
        }
    }

    fn applied_of(items: &[DeviceAssignment]) -> HashMap<String, DeviceAssignment> {
        items
            .iter()
            .map(|a| (a.drone_tag.clone(), a.clone()))
            .collect()
    }

    #[test]
    fn diff_detects_new_changed_and_removed() {
        let applied = applied_of(&[
            assignment("drone-a", "udpin:0.0.0.0:14540", 1),
            assignment("drone-b", "udpin:0.0.0.0:14541", 2),
        ]);
        let next = vec![
            assignment("drone-a", "udpin:0.0.0.0:14550", 1), // 端口变了
            assignment("drone-c", "udpin:0.0.0.0:14542", 3), // 新增
        ];

        let diff = diff_assignments(&applied, &next);
        let mut actions: Vec<(String, AssignmentAction)> = diff.actions;
        actions.sort_by(|a, b| a.0.cmp(&b.0));

        assert_eq!(
            actions,
            vec![
                ("drone-a".to_string(), AssignmentAction::Reconnect),
                ("drone-b".to_string(), AssignmentAction::Disconnect),
                ("drone-c".to_string(), AssignmentAction::Connect),
            ]
        );
    }

    #[test]
    fn diff_keeps_unchanged_assignment() {
        let applied = applied_of(&[assignment("drone-a", "udpin:0.0.0.0:14540", 1)]);
        let next = vec![assignment("drone-a", "udpin:0.0.0.0:14540", 1)];
        let diff = diff_assignments(&applied, &next);
        assert_eq!(
            diff.actions,
            vec![("drone-a".to_string(), AssignmentAction::Keep)]
        );
    }

    /// adapter 不消费的字段变化不触发重建。
    #[test]
    fn diff_ignores_fields_adapter_does_not_consume() {
        let mut old = assignment("drone-a", "udpin:0.0.0.0:14540", 1);
        let mut new = old.clone();
        old.signing_secret = "old-secret".into();
        new.signing_secret = "new-secret".into();
        new.allow_unsigned = !new.allow_unsigned;
        new.mavlink_version = 1;

        let applied = applied_of(&[old]);
        let diff = diff_assignments(&applied, &[new]);
        assert_eq!(
            diff.actions,
            vec![("drone-a".to_string(), AssignmentAction::Keep)]
        );
    }

    /// tracker 记录生效配置；替换整份快照后差量能识别出变化。
    #[test]
    fn tracker_tracks_applied_assignments() {
        let mut tracker = AssignmentTracker::new(&[assignment("drone-a", "udpin:0.0.0.0:1", 1)]);
        assert_eq!(tracker.applied().len(), 1);

        tracker.set_applied(
            "drone-b",
            assignment("drone-b", "udpin:0.0.0.0:2", 2),
        );
        assert_eq!(tracker.applied().len(), 2);

        tracker.clear_applied("drone-a");
        assert!(tracker.applied().get("drone-a").is_none());

        tracker.replace_all(&[assignment("drone-c", "udpin:0.0.0.0:3", 3)]);
        assert_eq!(tracker.applied().len(), 1);
        assert!(tracker.applied().get("drone-c").is_some());
    }
}
