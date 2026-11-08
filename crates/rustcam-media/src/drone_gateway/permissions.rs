//! 无人机接口的权限点定义与映射。
//!
//! REST 复用 JWT + Casbin，策略来自 `permissions` /`role_permissions` 表；
//! WS 用一次性票据。
//!
//! 权限名在数据库中形如 `drone:control:acquire`；Casbin 按**首个冒号**拆成
//! `(obj = "drone", act = "control:acquire")`，见 `auth::casbin::parse_permission`。

/// 权限点对象名。
pub const PERM_OBJECT: &str = "drone";

/// 查看无人机（设备列表、详情、控制权状态、告警、审计）。
pub const VIEW: &str = "view";
/// 读取遥测。
pub const TELEMETRY_READ: &str = "telemetry:read";
/// 获取控制权（含续约）。
pub const CONTROL_ACQUIRE: &str = "control:acquire";
/// 抢占控制权。
pub const CONTROL_PREEMPT: &str = "control:preempt";
/// 释放控制权。
pub const CONTROL_RELEASE: &str = "control:release";
/// 强制释放控制权（管理员）。
pub const CONTROL_FORCE_RELEASE: &str = "control:force_release";
/// 下发指令。
pub const COMMAND_SEND: &str = "command:send";
/// 管理无人机档案、航线与围栏。
pub const MANAGE: &str = "manage";

/// 需要在 `permissions` 表中补齐的权限点：(权限名, 描述)。
pub const ALL_PERMISSIONS: &[(&str, &str)] = &[
    ("drone:view", "查看无人机"),
    ("drone:telemetry:read", "读取遥测"),
    ("drone:control:acquire", "获取控制权"),
    ("drone:control:preempt", "抢占控制权"),
    ("drone:control:release", "释放控制权"),
    ("drone:control:force_release", "强制释放控制权"),
    ("drone:command:send", "下发指令"),
    ("drone:manage", "管理无人机档案、航线与围栏"),
];

/// WS 路径允许的消息类型。
pub fn ws_msg_types(kind: &str) -> &'static [&'static str] {
    match kind {
        "telemetry" => &["telemetry"],
        "alerts" => &["alert"],
        "control" => &["control"],
        "commands" => &["command"],
        "events" => &["telemetry", "alert", "control", "command"],
        _ => &[],
    }
}

/// WS 路径是否受支持。
pub fn is_ws_kind(kind: &str) -> bool {
    !ws_msg_types(kind).is_empty()
}

/// 路由路径前缀。
pub const ROUTE_PREFIX: &str = "/api/v1/drone";

/// 由匹配到的路由模式与 HTTP 方法推出可接受的权限点集合（任一命中即放行）。
///
/// 返回 `None` 表示该路由没有登记权限点；调用方必须按**拒绝**处理，
/// 避免新增路由时因漏配而裸奔。
pub fn required_permissions(path: &str, method: &str) -> Option<&'static [&'static str]> {
    const VIEW_ONLY: &[&str] = &[VIEW];
    const MANAGE_ONLY: &[&str] = &[MANAGE];
    match (path, method) {
        ("/api/v1/drone/ping", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/drones", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/drones", "POST") => Some(MANAGE_ONLY),
        ("/api/v1/drone/drones/{drone_tag}", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/drones/{drone_tag}", "PUT") => Some(MANAGE_ONLY),
        ("/api/v1/drone/drones/{drone_tag}", "DELETE") => Some(MANAGE_ONLY),
        ("/api/v1/drone/drones/{drone_tag}/cameras", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/drones/{drone_tag}/cameras", "POST") => Some(MANAGE_ONLY),
        (
            "/api/v1/drone/drones/{drone_tag}/cameras/{camera_device_tag}",
            "DELETE",
        ) => Some(MANAGE_ONLY),
        ("/api/v1/drone/devices", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/devices/{drone_tag}", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/{drone_tag}/command", "POST") => Some(&[COMMAND_SEND]),
        ("/api/v1/drone/{drone_tag}/telemetry", "GET") => Some(&[TELEMETRY_READ]),
        ("/api/v1/drone/devices/{drone_tag}/telemetry", "GET") => Some(&[TELEMETRY_READ]),
        ("/api/v1/drone/commands/{cmd_id}", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/{drone_tag}/commands/{cmd_id}", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/routes", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/routes", "POST") => Some(MANAGE_ONLY),
        ("/api/v1/drone/routes/{id}", "PUT") => Some(MANAGE_ONLY),
        ("/api/v1/drone/routes/{id}", "DELETE") => Some(MANAGE_ONLY),
        ("/api/v1/drone/routes/{id}/execute", "POST") => Some(&[COMMAND_SEND]),
        ("/api/v1/drone/fences", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/fences", "POST") => Some(MANAGE_ONLY),
        ("/api/v1/drone/fences/{id}", "PUT") => Some(MANAGE_ONLY),
        ("/api/v1/drone/fences/{id}", "DELETE") => Some(MANAGE_ONLY),
        ("/api/v1/drone/geofences", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/geofences", "POST") => Some(MANAGE_ONLY),
        ("/api/v1/drone/geofences/{id}", "PUT") => Some(MANAGE_ONLY),
        ("/api/v1/drone/geofences/{id}", "DELETE") => Some(MANAGE_ONLY),
        ("/api/v1/drone/alerts", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/audits", "GET") => Some(VIEW_ONLY),
        ("/api/v1/drone/replay", "GET") => Some(&[TELEMETRY_READ]),
        ("/api/v1/drone/control/acquire", "POST") => Some(&[CONTROL_ACQUIRE]),
        ("/api/v1/drone/control/preempt", "POST") => Some(&[CONTROL_PREEMPT]),
        ("/api/v1/drone/control/release", "POST") => Some(&[CONTROL_RELEASE]),
        ("/api/v1/drone/control/renew", "POST") => Some(&[CONTROL_ACQUIRE]),
        ("/api/v1/drone/control/force_release", "POST") => Some(&[CONTROL_FORCE_RELEASE]),
        ("/api/v1/drone/control/{drone_tag}/{control_type}", "GET") => Some(VIEW_ONLY),
        // 票据只用于订阅事件流：读权限或遥测权限任一即可换取，
        // 实际可订阅的消息类型由票据 scope 决定。
        ("/api/v1/drone/ws-ticket", "POST") => Some(&[VIEW, TELEMETRY_READ]),
        _ => None,
    }
}

/// 判断路径是否属于无人机 REST 接口。
pub fn is_drone_path(path: &str) -> bool {
    path.starts_with(ROUTE_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_path_maps_to_single_msg_type() {
        assert_eq!(ws_msg_types("telemetry"), &["telemetry"]);
        assert_eq!(ws_msg_types("alerts"), &["alert"]);
        assert_eq!(ws_msg_types("control"), &["control"]);
        assert_eq!(ws_msg_types("commands"), &["command"]);
        assert_eq!(ws_msg_types("events").len(), 4);
        assert!(ws_msg_types("unknown").is_empty());
    }

    #[test]
    fn all_permissions_cover_rule_required_points() {
        for required in [
            "drone:view",
            "drone:telemetry:read",
            "drone:control:acquire",
            "drone:control:preempt",
            "drone:control:release",
            "drone:control:force_release",
            "drone:command:send",
        ] {
            assert!(
                ALL_PERMISSIONS.iter().any(|(name, _)| *name == required),
                "缺少规则要求的权限点 {required}"
            );
        }
    }

    #[test]
    fn permission_names_split_into_drone_object() {
        for (name, _) in ALL_PERMISSIONS {
            let (obj, act) = name.split_once(':').expect("权限名必须是 obj:act");
            assert_eq!(obj, PERM_OBJECT);
            assert!(!act.is_empty());
        }
    }

    #[test]
    fn every_drone_route_has_a_permission() {
        let routes = [
            ("/api/v1/drone/ping", "GET"),
            ("/api/v1/drone/drones", "GET"),
            ("/api/v1/drone/drones", "POST"),
            ("/api/v1/drone/drones/{drone_tag}", "GET"),
            ("/api/v1/drone/drones/{drone_tag}", "PUT"),
            ("/api/v1/drone/drones/{drone_tag}", "DELETE"),
            ("/api/v1/drone/drones/{drone_tag}/cameras", "GET"),
            ("/api/v1/drone/drones/{drone_tag}/cameras", "POST"),
            (
                "/api/v1/drone/drones/{drone_tag}/cameras/{camera_device_tag}",
                "DELETE",
            ),
            ("/api/v1/drone/devices", "GET"),
            ("/api/v1/drone/devices/{drone_tag}", "GET"),
            ("/api/v1/drone/{drone_tag}/command", "POST"),
            ("/api/v1/drone/{drone_tag}/telemetry", "GET"),
            ("/api/v1/drone/devices/{drone_tag}/telemetry", "GET"),
            ("/api/v1/drone/commands/{cmd_id}", "GET"),
            ("/api/v1/drone/{drone_tag}/commands/{cmd_id}", "GET"),
            ("/api/v1/drone/routes", "GET"),
            ("/api/v1/drone/routes", "POST"),
            ("/api/v1/drone/routes/{id}", "PUT"),
            ("/api/v1/drone/routes/{id}", "DELETE"),
            ("/api/v1/drone/routes/{id}/execute", "POST"),
            ("/api/v1/drone/fences", "GET"),
            ("/api/v1/drone/fences", "POST"),
            ("/api/v1/drone/fences/{id}", "PUT"),
            ("/api/v1/drone/fences/{id}", "DELETE"),
            ("/api/v1/drone/geofences", "GET"),
            ("/api/v1/drone/geofences", "POST"),
            ("/api/v1/drone/geofences/{id}", "PUT"),
            ("/api/v1/drone/geofences/{id}", "DELETE"),
            ("/api/v1/drone/alerts", "GET"),
            ("/api/v1/drone/audits", "GET"),
            ("/api/v1/drone/control/acquire", "POST"),
            ("/api/v1/drone/control/preempt", "POST"),
            ("/api/v1/drone/control/release", "POST"),
            ("/api/v1/drone/control/renew", "POST"),
            ("/api/v1/drone/control/force_release", "POST"),
            ("/api/v1/drone/control/{drone_tag}/{control_type}", "GET"),
            ("/api/v1/drone/ws-ticket", "POST"),
        ];
        for (path, method) in routes {
            assert!(
                required_permissions(path, method).is_some(),
                "路由 {method} {path} 未登记权限点"
            );
        }
    }

    #[test]
    fn routes_use_drone_tag_path_params() {
        for (path, method) in [
            ("/api/v1/drone/drones/{drone_tag}", "GET"),
            ("/api/v1/drone/devices/{drone_tag}", "GET"),
            ("/api/v1/drone/{drone_tag}/command", "POST"),
            ("/api/v1/drone/{drone_tag}/telemetry", "GET"),
            ("/api/v1/drone/devices/{drone_tag}/telemetry", "GET"),
            ("/api/v1/drone/{drone_tag}/commands/{cmd_id}", "GET"),
            ("/api/v1/drone/control/{drone_tag}/{control_type}", "GET"),
        ] {
            assert!(
                required_permissions(path, method).is_some(),
                "路由 {method} {path} 未按 drone_tag 登记权限点"
            );
        }
    }

    #[test]
    fn unknown_route_has_no_permission() {
        assert!(required_permissions("/api/v1/drone/unknown", "GET").is_none());
        assert!(required_permissions("/api/v1/drone/ping", "DELETE").is_none());
        assert!(is_drone_path("/api/v1/drone/ping"));
        assert!(!is_drone_path("/api/v1/devices"));
    }
}
