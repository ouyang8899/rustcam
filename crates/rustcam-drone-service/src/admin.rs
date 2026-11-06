//! 产品管理面：无人机档案、航线、围栏，以及航线执行编排。
//!
//! 档案与航线属于业务数据（可先于设备上线创建）；航线执行复用既有指令状态机，
//! 顺序下发 `goto`，失败即中止并写审计。

use rustcam_drone_core::command::{is_terminal_status, status};
use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::{
    DroneProfile, Geofence as ProtoFence, Route as ProtoRoute, RouteWaypoint,
};

use crate::state::{
    ActiveMission, AuditRecord, DroneProfileRecord, DroneState, FenceRecord, RouteRecord,
    WaypointRecord,
};
use crate::{commands, control, geofence};

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn audit(
    state: &DroneState,
    device_id: &str,
    category: &str,
    action: &str,
    operator: &str,
    result: &str,
    detail: &str,
) {
    state.push_audit(AuditRecord {
        ts_ms: now_ms(),
        device_id: device_id.to_string(),
        category: category.to_string(),
        action: action.to_string(),
        operator: operator.to_string(),
        result: result.to_string(),
        detail: detail.to_string(),
        ..Default::default()
    });
    state.persist();
}

// ---------------------------------------------------------------------------
// 无人机档案
// ---------------------------------------------------------------------------

pub fn list_profiles(state: &DroneState) -> Vec<DroneProfileRecord> {
    let mut profiles: Vec<_> = state.profiles.iter().map(|e| e.value().clone()).collect();
    profiles.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    profiles
}

fn resolve_drone_tag(
    state: &DroneState,
    device_id: &str,
    incoming: &str,
) -> Result<String, String> {
    let existing = state
        .profiles
        .get(device_id)
        .map(|p| p.drone_tag.trim().to_string())
        .filter(|t| !t.is_empty());
    let tag = existing
        .or_else(|| {
            let t = incoming.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .unwrap_or_else(|| format!("drone-{}", uuid::Uuid::new_v4()));
    if let Some(other) = state
        .profiles
        .iter()
        .find(|e| e.key() != device_id && e.value().drone_tag.trim() == tag)
    {
        return Err(format!("drone_tag `{tag}` 已被 `{}` 使用", other.key()));
    }
    Ok(tag)
}

pub fn profile_to_proto(p: &DroneProfileRecord) -> DroneProfile {
    DroneProfile {
        device_id: p.device_id.clone(),
        name: p.name.clone(),
        model: p.model.clone(),
        autopilot: p.autopilot.clone(),
        device_tag: p.device_tag.clone(),
        channel_tag: p.channel_tag.clone(),
        video_stream: p.video_stream.clone(),
        notes: p.notes.clone(),
        enabled: p.enabled,
        created_ms: p.created_ms,
        updated_ms: p.updated_ms,
        operator: String::new(),
        unique_id: p.unique_id.clone(),
        mavlink_addr: p.mavlink_addr.clone(),
        signing_secret: p.signing_secret.clone(),
        signing_allow_unsigned: p.signing_allow_unsigned,
        mavlink_version: p.mavlink_version,
        mavlink_sysid: p.mavlink_sysid,
        mavlink_compid: p.mavlink_compid,
        adapter_id: p.adapter_id.clone(),
        drone_tag: p.drone_tag.clone(),
    }
}

pub fn upsert_profile(
    state: &DroneState,
    profile: DroneProfile,
    operator: &str,
) -> Result<DroneProfileRecord, String> {
    let device_id = profile.device_id.trim();
    if device_id.is_empty() {
        return Err("device_id is required".into());
    }
    let autopilot = profile.autopilot.trim().to_ascii_lowercase();
    if !matches!(
        autopilot.as_str(),
        "" | "auto" | "px4" | "ardupilot" | "apm"
    ) {
        return Err(format!("unsupported autopilot `{autopilot}`"));
    }

    let now = now_ms();
    let created_ms = state
        .profiles
        .get(device_id)
        .map(|p| p.created_ms)
        .unwrap_or(now);

    // 机载链路身份字段（drone.md §12.4 L3）。
    let unique_id = profile.unique_id.trim().to_ascii_lowercase();
    if !unique_id.is_empty() && !unique_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("unique_id must be lowercase hex".into());
    }
    let mavlink_version = if profile.mavlink_version == 0 {
        2
    } else {
        profile.mavlink_version
    };
    if mavlink_version != 1 && mavlink_version != 2 {
        return Err(format!("unsupported mavlink_version `{mavlink_version}`"));
    }
    if !(1..=255).contains(&profile.mavlink_sysid) {
        return Err(format!("invalid mavlink_sysid `{}`", profile.mavlink_sysid));
    }
    if !(1..=255).contains(&profile.mavlink_compid) {
        return Err(format!(
            "invalid mavlink_compid `{}`",
            profile.mavlink_compid
        ));
    }
    // 签名密钥以密文入库；没有主密钥时拒绝保存明文密钥。
    let secret_plain = profile.signing_secret.trim().to_string();
    if !secret_plain.is_empty() && mavlink_version == 1 {
        return Err("MAVLink 1 has no signature field: clear signing_secret".into());
    }
    let signing_secret = if secret_plain.is_empty() {
        String::new()
    } else {
        let master = state
            .config
            .signing_master_key
            .as_deref()
            .ok_or_else(|| "DRONE_SIGNING_MASTER_KEY is not configured".to_string())?;
        crate::secret::SecretBox::from_hex(master)
            .and_then(|sb| sb.seal(&secret_plain))
            .map_err(|e| format!("seal signing secret failed: {e}"))?
    };

    let drone_tag = resolve_drone_tag(state, device_id, &profile.drone_tag)?;

    let record = DroneProfileRecord {
        device_id: device_id.to_string(),
        name: if profile.name.trim().is_empty() {
            device_id.to_string()
        } else {
            profile.name.trim().to_string()
        },
        model: profile.model.trim().to_string(),
        autopilot: if autopilot.is_empty() {
            "auto".into()
        } else if autopilot == "apm" {
            "ardupilot".into()
        } else {
            autopilot
        },
        device_tag: profile.device_tag.trim().to_string(),
        channel_tag: profile.channel_tag.trim().to_string(),
        video_stream: profile.video_stream.trim().to_string(),
        notes: profile.notes.trim().to_string(),
        enabled: profile.enabled,
        created_ms,
        updated_ms: now,
        unique_id,
        mavlink_addr: profile.mavlink_addr.trim().to_string(),
        signing_secret,
        signing_allow_unsigned: profile.signing_allow_unsigned,
        mavlink_version,
        mavlink_sysid: profile.mavlink_sysid,
        mavlink_compid: profile.mavlink_compid,
        adapter_id: profile.adapter_id.trim().to_string(),
        drone_tag,
    };
    state
        .profiles
        .insert(record.device_id.clone(), record.clone());
    audit(
        state,
        device_id,
        "profile",
        "upsert",
        operator,
        "ok",
        &format!("name={}", record.name),
    );
    Ok(record)
}

/// 把机载链路参数写入 `drone_device` 表（drone.md §9.1）。
///
/// 档案在内存与快照文件里，链路参数以 drone 表为唯一真相源；
/// adapter 注册时按 `adapter_id` 从这里取端点与密钥下发。
pub async fn persist_link_params(
    state: &DroneState,
    record: &DroneProfileRecord,
) -> Result<(), String> {
    let Some(db) = state.db.clone() else {
        return Err("database is not configured".into());
    };
    let row = crate::infra::db::DeviceLinkRow {
        device_id: record.device_id.clone(),
        unique_id: record.unique_id.clone(),
        mavlink_addr: record.mavlink_addr.clone(),
        signing_secret: record.signing_secret.clone(),
        signing_allow_unsigned: record.signing_allow_unsigned,
        mavlink_version: record.mavlink_version,
        mavlink_sysid: record.mavlink_sysid,
        mavlink_compid: record.mavlink_compid,
        adapter_id: record.adapter_id.clone(),
        drone_tag: record.drone_tag.clone(),
    };
    db.upsert_device_link(&row)
        .await
        .map_err(|e| format!("persist link params failed: {e}"))
}

pub async fn delete_profile(
    state: &DroneState,
    device_id: &str,
    operator: &str,
) -> Result<bool, String> {
    let existed = state.profiles.remove(device_id).is_some();
    if existed {
        // 多对多航线：只移除该无人机；若它是唯一适用机则删除航线。
        let affected: Vec<(String, RouteRecord)> = state
            .routes
            .iter()
            .filter(|r| {
                let v = r.value();
                v.device_id == device_id || v.device_ids.iter().any(|id| id == device_id)
            })
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();
        for (id, mut route) in affected {
            let mut delete = route.device_ids.is_empty();
            if !delete {
                route.device_ids.retain(|x| x != device_id);
                delete = route.device_ids.is_empty();
            }
            if delete {
                if let Some(db) = state.db.clone() {
                    db.delete_route(&id)
                        .await
                        .map_err(|e| format!("航线删除入库失败：{e}"))?;
                }
                state.routes.remove(&id);
                continue;
            }
            route.device_id = if route.device_ids.len() == 1 {
                route.device_ids[0].clone()
            } else {
                String::new()
            };
            if let Some(db) = state.db.clone() {
                db.upsert_route(&route)
                    .await
                    .map_err(|e| format!("航线入库失败：{e}"))?;
            }
            state.routes.insert(id, route);
        }
        audit(state, device_id, "profile", "delete", operator, "ok", "");
    }
    Ok(existed)
}

// ---------------------------------------------------------------------------
// 航线
// ---------------------------------------------------------------------------

pub fn list_routes(state: &DroneState, device_id: &str) -> Vec<RouteRecord> {
    let mut routes: Vec<_> = state
        .routes
        .iter()
        .filter(|r| {
            let v = r.value();
            if device_id.is_empty() {
                return true;
            }
            if !v.device_ids.is_empty() {
                v.device_ids.iter().any(|id| id == device_id)
            } else {
                // 旧数据没有 device_ids：单机航线按 device_id，空值视为模板。
                v.device_id.is_empty() || v.device_id == device_id
            }
        })
        .map(|e| e.value().clone())
        .collect();
    routes.sort_by(|a, b| a.updated_ms.cmp(&b.updated_ms).reverse());
    routes
}

pub fn route_to_proto(r: &RouteRecord) -> ProtoRoute {
    let device_ids = if r.device_ids.is_empty() && !r.device_id.trim().is_empty() {
        vec![r.device_id.clone()]
    } else {
        r.device_ids.clone()
    };
    ProtoRoute {
        id: r.id.clone(),
        name: r.name.clone(),
        device_id: if device_ids.len() == 1 {
            device_ids[0].clone()
        } else {
            String::new()
        },
        waypoints: r
            .waypoints
            .iter()
            .map(|w| RouteWaypoint {
                lat: w.lat,
                lon: w.lon,
                alt: w.alt,
                speed: w.speed,
                loiter_sec: w.loiter_sec,
                action: w.action.clone(),
            })
            .collect(),
        enabled: r.enabled,
        updated_ms: r.updated_ms,
        operator: String::new(),
        end_action: normalize_end_action(&r.end_action).to_string(),
        device_ids,
    }
}

/// 结束动作归一化：`hold`（默认）/ `rtl` / `land`。
pub fn normalize_end_action(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "rtl" | "return" => "rtl",
        "land" => "land",
        _ => "hold",
    }
}

fn route_end_action(state: &DroneState, route: &RouteRecord) -> &'static str {
    if route.end_action.trim().is_empty() {
        normalize_end_action(&state.config.route_default_end_action)
    } else {
        normalize_end_action(&route.end_action)
    }
}

pub async fn upsert_route(
    state: &DroneState,
    route: ProtoRoute,
    operator: &str,
) -> Result<RouteRecord, String> {
    if route.name.trim().is_empty() {
        return Err("路线名称不能为空".into());
    }
    if route.waypoints.is_empty() {
        return Err("航线至少需要一个航点".into());
    }
    if route.waypoints.len() > 200 {
        return Err("单个航线最多 200 个航点".into());
    }
    for (i, w) in route.waypoints.iter().enumerate() {
        if !(-90.0..=90.0).contains(&w.lat) || !(-180.0..=180.0).contains(&w.lon) {
            return Err(format!("第 {} 个航点坐标非法", i + 1));
        }
        if !(0.0..=500.0).contains(&w.alt) {
            return Err(format!("第 {} 个航点高度越界", i + 1));
        }
        if !matches!(w.action.as_str(), "" | "waypoint" | "takeoff" | "land") {
            return Err(format!("第 {} 个航点动作不支持", i + 1));
        }
    }

    let id = if route.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        route.id.trim().to_string()
    };
    let mut device_ids: Vec<String> = route
        .device_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // 兼容旧客户端只传 device_id。
    if device_ids.is_empty() && !route.device_id.trim().is_empty() {
        device_ids.push(route.device_id.trim().to_string());
    }
    device_ids.sort();
    device_ids.dedup();
    let end_action = if route.end_action.trim().is_empty() {
        normalize_end_action(&state.config.route_default_end_action).to_string()
    } else {
        normalize_end_action(&route.end_action).to_string()
    };
    let record = RouteRecord {
        id: id.clone(),
        name: route.name.trim().to_string(),
        device_id: if device_ids.len() == 1 {
            device_ids[0].clone()
        } else {
            String::new()
        },
        device_ids,
        waypoints: route
            .waypoints
            .iter()
            .map(|w| WaypointRecord {
                lat: w.lat,
                lon: w.lon,
                alt: w.alt,
                speed: w.speed,
                loiter_sec: w.loiter_sec,
                action: if w.action.is_empty() {
                    "waypoint".into()
                } else {
                    w.action.clone()
                },
            })
            .collect(),
        enabled: route.enabled,
        updated_ms: now_ms(),
        end_action,
    };
    // PostgreSQL 为权威源：先入库，成功后再更新内存状态。
    if let Some(db) = state.db.clone() {
        db.upsert_route(&record)
            .await
            .map_err(|e| format!("航线入库失败：{e}"))?;
    }
    state.routes.insert(id.clone(), record.clone());
    audit(
        state,
        &record.device_id,
        "route",
        "upsert",
        operator,
        "ok",
        &format!(
            "route_id={id} waypoints={} devices={}",
            record.waypoints.len(),
            record.device_ids.len()
        ),
    );
    Ok(record)
}

pub async fn delete_route(state: &DroneState, id: &str, operator: &str) -> Result<bool, String> {
    if let Some(db) = state.db.clone() {
        db.delete_route(id)
            .await
            .map_err(|e| format!("航线删除入库失败：{e}"))?;
    }
    let existed = state.routes.remove(id).is_some();
    if existed {
        audit(state, "", "route", "delete", operator, "ok", id);
    }
    Ok(existed)
}

/// 执行航线：任务编排全部在服务端完成。
///
/// - 校验航线、围栏与控制权；
/// - 按任务预计时长校验并延长控制权租约（不依赖前端续约）；
/// - 未解锁时先自动起飞，再按航点顺序执行；
/// - 完成后按航线自身的 `end_action`（hold / rtl / land）执行。
///
/// 调用方立即拿到“任务第一条指令”的回执，后续步骤在后台任务中执行。
pub async fn execute_route(
    state: &DroneState,
    route_id: &str,
    device_id: &str,
    lateral_offset_m: f64,
    operator: &str,
) -> Result<proto::CommandReply, String> {
    let route = state
        .routes
        .get(route_id)
        .map(|r| r.value().clone())
        .ok_or_else(|| format!("航线 `{route_id}` 不存在"))?;
    if !route.enabled {
        return Err("航线已禁用".into());
    }
    if operator.trim().is_empty() {
        return Err("operator is required".into());
    }
    // 群控暂不开放：一次任务必须明确指定唯一一架无人机，绝不自动推断/批量执行。
    let target = device_id.trim();
    if target.is_empty() {
        return Err("一次只允许执行一架无人机，请指定执行目标；多架请逐架执行".into());
    }
    let target = target.to_string();
    let applies = if !route.device_ids.is_empty() {
        route.device_ids.iter().any(|id| id == &target)
    } else {
        route.device_id.is_empty() || route.device_id == target
    };
    if !applies {
        return Err(format!("航线 `{}` 不适用于无人机 `{target}`", route.name));
    }
    if !lateral_offset_m.is_finite() || lateral_offset_m.abs() > 200.0 {
        return Err("横向偏移必须在 ±200 米以内".into());
    }
    // 本次任务的航点：按航线走向整体左右平移，原航线本身不变。
    let mission_route = offset_route(&route, lateral_offset_m);
    control::ensure_holder(state, &target, control::CONTROL_FLIGHT, operator).await?;
    for (index, waypoint) in mission_route.waypoints.iter().enumerate() {
        geofence::check_target(
            state,
            &target,
            waypoint.lat,
            waypoint.lon,
            waypoint.alt,
        )
        .map_err(|e| format!("第 {} 个航点不可达：{e}", index + 1))?;
    }

    let telemetry = latest_telemetry(state, &target).filter(|t| t.has_position());
    let Some(telemetry) = telemetry else {
        return Err("无人机当前位置未知（缺少有效定位遥测），无法规划转场与航线执行".into());
    };
    let end_action = route_end_action(state, &route);
    let estimate = route_estimate_seconds(&mission_route, Some(&telemetry), end_action);
    let transit_m = geofence::haversine_m(
        telemetry.lat,
        telemetry.lon,
        mission_route.waypoints[0].lat,
        mission_route.waypoints[0].lon,
    );
    if estimate + 120 > 3600 {
        return Err(format!(
            "航线预计 {estimate} 秒（含转场 {} 米），超过单次控制权租约上限 3600 秒，请拆分航线",
            transit_m.round()
        ));
    }

    // 多机同空域互斥：在真正下发指令前占用任务槽，冲突则拒绝。
    let reservation = reserve_mission(state, &target, &mission_route, estimate)?;

    // 任务可能持续十几分钟：租约必须覆盖“任务 + 返航 + 余量”，由后端统一延长。
    let covered = control::get(state, &target, control::CONTROL_FLIGHT)
        .await
        .map(|lease| lease.expire_at_ms - now_ms() >= (estimate + 120) * 1000)
        .unwrap_or(false);
    if !covered {
        let need = (estimate + 180).min(3600) as i32;
        control::renew(
            state,
            &target,
            control::CONTROL_FLIGHT,
            operator,
            need,
        )
        .await
        .map_err(|e| format!("任务预计 {estimate} 秒，自动延长控制权租约失败：{e}"))?;
    }

    let plan: Vec<proto::Command> = (0..mission_route.waypoints.len())
        .map(|index| build_goto(&mission_route, index, operator, &target))
        .collect();
    let first_alt = mission_route.waypoints[0].alt.max(5.0);
    let airborne = telemetry.armed && telemetry.rel_alt > 2.0;

    let (reply, first_already_sent, wait_takeoff) = if airborne {
        let reply = commands::submit(state, plan[0].clone()).await;
        if !mission_started(&reply) {
            return Ok(reply);
        }
        (reply, true, false)
    } else {
        let reply = commands::submit(
            state,
            build_takeoff(&mission_route, first_alt, operator, &target),
        )
        .await;
        if !mission_started(&reply) {
            return Ok(reply);
        }
        (reply, false, true)
    };

    audit(
        state,
        &target,
        "route",
        "execute",
        operator,
        "started",
        &format!(
            "route_id={route_id} estimate={estimate}s transit={:.0}m lateral_offset={lateral_offset_m:.1}m auto_takeoff={} end_action={end_action}",
            transit_m, !airborne
        ),
    );
    // 任务已启动：活动任务槽随后台任务生命周期释放。
    let state = state.clone();
    let operator = operator.to_string();
    tokio::spawn(async move {
        let _reservation = reservation;
        run_route(
            state,
            mission_route,
            target,
            operator,
            plan,
            first_already_sent,
            wait_takeoff,
            first_alt,
        )
        .await;
    });
    Ok(reply)
}

fn mission_started(reply: &proto::CommandReply) -> bool {
    matches!(
        reply.status.as_str(),
        status::SENT | status::PENDING | status::ACCEPTED
    )
}

/// 任务启动失败或任务结束时释放活动任务槽。
struct MissionReservation {
    state: DroneState,
    device_id: String,
}

impl Drop for MissionReservation {
    fn drop(&mut self) {
        self.state.active_missions.remove(&self.device_id);
    }
}

/// 多机同空域互斥：同一架无人机只能有一个任务；不同无人机的航点
/// 水平 <50m 且高差 <15m 视为冲突，直接拒绝启动，避免空中相撞。
fn reserve_mission(
    state: &DroneState,
    device_id: &str,
    route: &RouteRecord,
    estimate: i64,
) -> Result<MissionReservation, String> {
    let _lock = state
        .mission_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let now = now_ms();
    let expired: Vec<String> = state
        .active_missions
        .iter()
        .filter(|entry| entry.value().end_ms < now)
        .map(|entry| entry.key().clone())
        .collect();
    for key in expired {
        state.active_missions.remove(&key);
    }
    if let Some(entry) = state.active_missions.get(device_id) {
        return Err(format!(
            "无人机 `{device_id}` 已有任务（航线 `{}`）在执行",
            entry.route_id
        ));
    }
    for entry in state.active_missions.iter() {
        let mission = entry.value();
        for waypoint in &route.waypoints {
            for (active_lat, active_lon, active_alt) in &mission.waypoints {
                if geofence::haversine_m(
                    waypoint.lat,
                    waypoint.lon,
                    *active_lat,
                    *active_lon,
                ) < 15.0
                    && (waypoint.alt - *active_alt).abs() < 10.0
                {
                    return Err(format!(
                        "空域冲突：无人机 `{}` 正在执行航线 `{}`，与本任务航点过近（水平 <15m、高差 <10m），请加横向偏移或等待其完成",
                        entry.key(),
                        mission.route_id
                    ));
                }
            }
        }
    }
    state.active_missions.insert(
        device_id.to_string(),
        ActiveMission {
            route_id: route.id.clone(),
            waypoints: route
                .waypoints
                .iter()
                .map(|w| (w.lat, w.lon, w.alt))
                .collect(),
            end_ms: now + estimate.max(0) * 1000 + 300_000,
        },
    );
    Ok(MissionReservation {
        state: state.clone(),
        device_id: device_id.to_string(),
    })
}

fn latest_telemetry(state: &DroneState, device_id: &str) -> Option<Telemetry> {
    state
        .devices
        .get(device_id)
        .and_then(|entry| entry.last_telemetry.clone())
}

/// 本次任务的航点：按航线走向整体左右平移（正=右，负=左）。
///
/// 只改变本次执行的计划，不修改保存的航线；用于同航线多机横向分道。
fn offset_route(route: &RouteRecord, lateral_m: f64) -> RouteRecord {
    let mut out = route.clone();
    let n = route.waypoints.len();
    if lateral_m.abs() < 0.01 || n < 2 {
        return out;
    }
    let mut waypoints = Vec::with_capacity(n);
    for (i, waypoint) in route.waypoints.iter().enumerate() {
        let (from, to) = if i == 0 {
            (&route.waypoints[0], &route.waypoints[1])
        } else if i == n - 1 {
            (&route.waypoints[n - 2], &route.waypoints[n - 1])
        } else {
            (&route.waypoints[i - 1], &route.waypoints[i + 1])
        };
        let bearing = bearing_deg(from.lat, from.lon, to.lat, to.lon);
        let dir = bearing + if lateral_m >= 0.0 { 90.0 } else { -90.0 };
        let d = lateral_m.abs();
        let dlat = d * dir.to_radians().cos() / 111_320.0;
        let dlon =
            d * dir.to_radians().sin() / (111_320.0 * waypoint.lat.to_radians().cos().abs().max(0.1));
        waypoints.push(WaypointRecord {
            lat: waypoint.lat + dlat,
            lon: waypoint.lon + dlon,
            ..waypoint.clone()
        });
    }
    out.waypoints = waypoints;
    out
}

/// 起点到终点的方位角（度，正北为 0）。
fn bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let delta_lon = (lon2 - lon1).to_radians();
    let y = delta_lon.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * delta_lon.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// 任务预计耗时（秒）：起飞余量 + 各航段 + 悬停 + 可选返航段。
fn route_estimate_seconds(
    route: &RouteRecord,
    telemetry: Option<&Telemetry>,
    end_action: &str,
) -> i64 {
    let first = &route.waypoints[0];
    let mut lat = telemetry.map(|t| t.lat).unwrap_or(first.lat);
    let mut lon = telemetry.map(|t| t.lon).unwrap_or(first.lon);
    let mut total = 30.0;
    for waypoint in &route.waypoints {
        let speed = if waypoint.speed.is_finite() && waypoint.speed > 0.5 {
            waypoint.speed.clamp(1.0, 30.0)
        } else {
            8.0
        };
        total += geofence::haversine_m(lat, lon, waypoint.lat, waypoint.lon) / speed;
        total += waypoint.loiter_sec.max(0) as f64;
        lat = waypoint.lat;
        lon = waypoint.lon;
    }
    if end_action == "rtl" {
        let (home_lat, home_lon) = telemetry
            .filter(|t| t.home_lat != 0.0 && t.home_lon != 0.0)
            .map(|t| (t.home_lat, t.home_lon))
            .unwrap_or((first.lat, first.lon));
        total += geofence::haversine_m(lat, lon, home_lat, home_lon) / 8.0 + 30.0;
    } else if end_action == "land" {
        total += 30.0;
    }
    total.ceil() as i64
}

fn build_takeoff(route: &RouteRecord, alt: f64, operator: &str, device_id: &str) -> proto::Command {
    let mut params = std::collections::HashMap::new();
    params.insert("alt".to_string(), alt.max(5.0).to_string());
    proto::Command {
        cmd_id: format!("{}-takeoff-{}", route.id, uuid::Uuid::new_v4().simple()),
        device_id: device_id.to_string(),
        r#type: "takeoff".into(),
        params,
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

fn build_rtl(route: &RouteRecord, operator: &str, device_id: &str) -> proto::Command {
    proto::Command {
        cmd_id: format!("{}-rtl-{}", route.id, uuid::Uuid::new_v4().simple()),
        device_id: device_id.to_string(),
        r#type: "rtl".into(),
        params: std::collections::HashMap::new(),
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

fn build_land(route: &RouteRecord, operator: &str, device_id: &str) -> proto::Command {
    proto::Command {
        cmd_id: format!("{}-land-{}", route.id, uuid::Uuid::new_v4().simple()),
        device_id: device_id.to_string(),
        r#type: "land".into(),
        params: std::collections::HashMap::new(),
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

fn build_goto(
    route: &RouteRecord,
    index: usize,
    operator: &str,
    device_id: &str,
) -> proto::Command {
    let w = &route.waypoints[index];
    let mut params = std::collections::HashMap::new();
    params.insert("lat".to_string(), w.lat.to_string());
    params.insert("lon".to_string(), w.lon.to_string());
    params.insert("alt".to_string(), w.alt.to_string());
    proto::Command {
        cmd_id: format!("{}-{}", route.id, uuid::Uuid::new_v4().simple()),
        device_id: device_id.to_string(),
        r#type: "goto".into(),
        params,
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

/// 无人持有控制权时的安全返航：绕过控制权校验下发 RTL。
///
/// 只有在租约过期/被释放且没有新持有人时才调用；被抢占时交给新持有人，
/// 不做自动返航，避免和接管者的操作冲突。
async fn send_safety_rtl(
    state: &DroneState,
    route: &RouteRecord,
    device_id: &str,
    reason: &str,
) {
    let mut cmd = build_rtl(route, "system", device_id);
    cmd.cmd_id = format!("{}-safety-rtl-{}", route.id, uuid::Uuid::new_v4().simple());
    let reply = commands::submit_safety(state, cmd).await;
    let result = if mission_started(&reply) {
        "ok"
    } else {
        reply.status.as_str()
    };
    audit(
        state,
        &device_id,
        "route",
        "safety_rtl",
        "system",
        result,
        &format!("route_id={} reason={reason} status={}", route.id, reply.status),
    );
}

async fn run_route(
    state: DroneState,
    route: RouteRecord,
    device_id: String,
    operator: String,
    plan: Vec<proto::Command>,
    first_already_sent: bool,
    wait_takeoff: bool,
    takeoff_alt: f64,
) {
    for (index, _) in route.waypoints.iter().enumerate() {
        // 航线可能持续十几分钟，期间租约过期/被抢占时必须停止继续下发，
        // 不能让任务在“已失去控制权”的状态下悄悄跑完。
        if let Err(e) = control::ensure_holder(
            &state,
            &device_id,
            control::CONTROL_FLIGHT,
            &operator,
        )
        .await
        {
            // 区分“被接管”和“无人持有”：被接管时交给新持有人；无人持有时安全返航。
            if let Some(lease) =
                control::get(&state, &device_id, control::CONTROL_FLIGHT).await
            {
                audit(
                    &state,
                    &device_id,
                    "route",
                    "abort",
                    &operator,
                    "control_lost",
                    &format!(
                        "route_id={} waypoint={} new_holder={} {e}",
                        route.id,
                        index + 1,
                        lease.holder
                    ),
                );
                return;
            }
            audit(
                &state,
                &device_id,
                "route",
                "abort",
                &operator,
                "control_expired",
                &format!("route_id={} waypoint={} {e}", route.id, index + 1),
            );
            send_safety_rtl(&state, &route, &device_id, "control_expired").await;
            return;
        }

        // 未解锁时由服务端先发起飞指令，等离地后再开始航点。
        if index == 0
            && wait_takeoff
            && !wait_airborne(
                &state,
                &device_id,
                takeoff_alt,
                std::time::Duration::from_secs(60),
            )
            .await
        {
            audit(
                &state,
                &device_id,
                "route",
                "abort",
                &operator,
                "takeoff_timeout",
                &format!("route_id={} takeoff_alt={takeoff_alt}", route.id),
            );
            return;
        }

        let Some(cmd) = plan.get(index).cloned() else {
            break;
        };
        if index > 0 || !first_already_sent {
            let reply = commands::submit(&state, cmd.clone()).await;
            if !mission_started(&reply) {
                audit(
                    &state,
                    &device_id,
                    "route",
                    "abort",
                    &operator,
                    &reply.status,
                    &format!(
                        "route_id={} waypoint={} {}",
                        route.id,
                        index + 1,
                        reply.message
                    ),
                );
                return;
            }
        }

        if !wait_command(&state, &cmd.cmd_id, std::time::Duration::from_secs(120)).await {
            audit(
                &state,
                &device_id,
                "route",
                "abort",
                &operator,
                "timeout",
                &format!("route_id={} waypoint={}", route.id, index + 1),
            );
            return;
        }

        // ACK 只代表飞控接收了 goto；必须等真正飞抵航点再下发下一个，
        // 否则后续 goto 会立刻覆盖目标点，无人机只会飞向最后一个航点。
        let waypoint = &route.waypoints[index];
        if !wait_reached(
            &state,
            &device_id,
            waypoint.lat,
            waypoint.lon,
            std::time::Duration::from_secs(WAYPOINT_REACH_TIMEOUT_SECS),
        )
        .await
        {
            audit(
                &state,
                &device_id,
                "route",
                "abort",
                &operator,
                "timeout",
                &format!("route_id={} waypoint={} 未到达", route.id, index + 1),
            );
            return;
        }

        if route.waypoints[index].loiter_sec > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(
                route.waypoints[index].loiter_sec as u64,
            ))
            .await;
        }
    }

    // 任务完成后的动作由航线自身配置决定：hold / rtl / land。
    let end_action = route_end_action(&state, &route);
    if end_action == "hold" {
        audit(
            &state,
            &device_id,
            "route",
            "complete",
            &operator,
            "ok",
            &format!("route_id={} end_action=hold", route.id),
        );
        return;
    }

    if let Err(e) =
        control::ensure_holder(&state, &device_id, control::CONTROL_FLIGHT, &operator).await
    {
        if control::get(&state, &device_id, control::CONTROL_FLIGHT)
            .await
            .is_some()
        {
            audit(
                &state,
                &device_id,
                "route",
                "complete",
                &operator,
                "control_lost",
                &format!(
                    "route_id={} end_action={end_action} skipped: {e}",
                    route.id
                ),
            );
        } else {
            audit(
                &state,
                &device_id,
                "route",
                "complete",
                &operator,
                "control_expired",
                &format!("route_id={} safety_rtl: {e}", route.id),
            );
            send_safety_rtl(&state, &route, &device_id, "control_expired_before_end_action").await;
        }
        return;
    }
    let cmd = if end_action == "rtl" {
        build_rtl(&route, &operator, &device_id)
    } else {
        build_land(&route, &operator, &device_id)
    };
    let reply = commands::submit(&state, cmd).await;
    let result = if mission_started(&reply) {
        "ok"
    } else {
        reply.status.as_str()
    };
    audit(
        &state,
        &device_id,
        "route",
        "complete",
        &operator,
        result,
        &format!(
            "route_id={} end_action={end_action} status={}",
            route.id, reply.status
        ),
    );
}

/// 等待起飞完成：解锁且相对高度达到目标的一个安全比例。
async fn wait_airborne(
    state: &DroneState,
    device_id: &str,
    target_alt: f64,
    timeout: std::time::Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    let need = (target_alt * 0.7).clamp(2.0, 10.0);
    loop {
        if let Some(t) = latest_telemetry(state, device_id) {
            if t.armed && t.rel_alt >= need {
                return true;
            }
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn wait_command(state: &DroneState, cmd_id: &str, timeout: std::time::Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(record) = state.commands.get(cmd_id) {
            if is_terminal_status(&record.status) {
                return matches!(record.status.as_str(), status::ACCEPTED | status::SENT);
            }
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
}

/// 航点到达判定：水平容差（米）与单航点最长等待时间。
///
/// 只判水平距离：高度由 goto 指令自己收敛，避免因不同飞控/仿真器的
/// 高度基准差异导致航线卡在“永远未到达”。
const WAYPOINT_REACH_RADIUS_M: f64 = 5.0;
const WAYPOINT_REACH_TIMEOUT_SECS: u64 = 900;

/// 轮询最新遥测，直到无人机进入航点水平容差范围或超时。
async fn wait_reached(
    state: &DroneState,
    device_id: &str,
    lat: f64,
    lon: f64,
    timeout: std::time::Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(entry) = state.devices.get(device_id) {
            if let Some(t) = entry.last_telemetry.as_ref() {
                if geofence::haversine_m(t.lat, t.lon, lat, lon) <= WAYPOINT_REACH_RADIUS_M {
                    return true;
                }
            }
        }
        if tokio::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

// ---------------------------------------------------------------------------
// 围栏
// ---------------------------------------------------------------------------

pub fn list_fences(state: &DroneState) -> Vec<FenceRecord> {
    let mut fences: Vec<_> = state.fences.iter().map(|e| e.value().clone()).collect();
    fences.sort_by(|a, b| a.id.cmp(&b.id));
    fences
}

pub fn fence_to_proto(f: &FenceRecord) -> ProtoFence {
    let device_ids = if f.device_ids.is_empty() && !f.device_id.trim().is_empty() {
        vec![f.device_id.clone()]
    } else {
        f.device_ids.clone()
    };
    ProtoFence {
        id: f.id.clone(),
        name: f.name.clone(),
        r#type: f.rule_type.clone(),
        geojson: f.geojson.clone(),
        min_alt: f.min_alt,
        max_alt: f.max_alt,
        enabled: f.enabled,
        version: f.version,
        center_lat: f.center_lat,
        center_lon: f.center_lon,
        radius_m: f.radius_m,
        device_id: if device_ids.len() == 1 {
            device_ids[0].clone()
        } else {
            String::new()
        },
        device_ids,
        shape: f.shape.clone(),
        updated_ms: f.updated_ms,
    }
}

pub async fn upsert_fence(
    state: &DroneState,
    fence: ProtoFence,
    operator: &str,
) -> Result<FenceRecord, String> {
    if fence.name.trim().is_empty() {
        return Err("围栏名称不能为空".into());
    }
    let rule_type = if fence.r#type.is_empty() {
        "no_fly".to_string()
    } else {
        // 规则 §5.4 / §6.10：no_fly / work / height（`allowed` 视作 work 的旧名）。
        crate::geofence::normalize_rule(&fence.r#type).to_string()
    };
    if !matches!(rule_type.as_str(), "no_fly" | "work" | "height") {
        return Err("围栏类型只能是 no_fly、work 或 height".into());
    }
    let shape = if fence.shape.is_empty() {
        "polygon".to_string()
    } else {
        fence.shape.trim().to_ascii_lowercase()
    };
    if !matches!(shape.as_str(), "polygon" | "circle") {
        return Err("围栏形状只能是 polygon 或 circle".into());
    }
    if shape == "circle" {
        if fence.radius_m <= 0.0 {
            return Err("圆形围栏半径必须大于 0".into());
        }
        if !(-90.0..=90.0).contains(&fence.center_lat)
            || !(-180.0..=180.0).contains(&fence.center_lon)
        {
            return Err("圆形围栏中心坐标非法".into());
        }
    } else {
        let value: serde_json::Value = serde_json::from_str(&fence.geojson)
            .map_err(|_| "围栏 GeoJSON 解析失败".to_string())?;
        let kind = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(
            kind,
            "Polygon" | "MultiPolygon" | "Feature" | "FeatureCollection"
        ) {
            return Err("围栏 GeoJSON 必须是 Polygon/MultiPolygon".into());
        }
    }
    if fence.max_alt > 0.0 && fence.max_alt < fence.min_alt {
        return Err("围栏最大高度不能小于最小高度".into());
    }

    let id = if fence.id.trim().is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        fence.id.trim().to_string()
    };
    let mut device_ids: Vec<String> = fence
        .device_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // 兼容旧客户端只传 device_id 的情况。
    if device_ids.is_empty() && !fence.device_id.trim().is_empty() {
        device_ids.push(fence.device_id.trim().to_string());
    }
    device_ids.sort();
    device_ids.dedup();
    let version = state.fences.get(&id).map(|f| f.version + 1).unwrap_or(1);
    let record = FenceRecord {
        id: id.clone(),
        name: fence.name.trim().to_string(),
        rule_type,
        shape,
        geojson: fence.geojson,
        center_lat: fence.center_lat,
        center_lon: fence.center_lon,
        radius_m: fence.radius_m,
        min_alt: fence.min_alt,
        max_alt: fence.max_alt,
        device_id: if device_ids.len() == 1 {
            device_ids[0].clone()
        } else {
            String::new()
        },
        device_ids,
        enabled: fence.enabled,
        version,
        updated_ms: now_ms(),
    };
    if let Some(db) = state.db.clone() {
        db.upsert_fence(&record)
            .await
            .map_err(|e| format!("围栏入库失败：{e}"))?;
    }
    state.fences.insert(id.clone(), record.clone());
    audit(
        state,
        &record.device_id,
        "fence",
        "upsert",
        operator,
        "ok",
        &format!("fence_id={id} name={}", record.name),
    );
    Ok(record)
}

pub async fn delete_fence(state: &DroneState, id: &str, operator: &str) -> Result<bool, String> {
    if let Some(db) = state.db.clone() {
        db.delete_fence(id)
            .await
            .map_err(|e| format!("围栏删除入库失败：{e}"))?;
    }
    let existed = state.fences.remove(id).is_some();
    if existed {
        audit(state, "", "fence", "delete", operator, "ok", id);
    }
    Ok(existed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;

    const MASTER: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn state_with_master() -> DroneState {
        let mut cfg = DroneConfig::default();
        cfg.signing_master_key = Some(MASTER.to_string());
        DroneState::new(cfg)
    }

    fn profile() -> DroneProfile {
        DroneProfile {
            device_id: "drone-001".into(),
            name: String::new(),
            model: String::new(),
            autopilot: "auto".into(),
            device_tag: String::new(),
            channel_tag: String::new(),
            video_stream: String::new(),
            notes: String::new(),
            enabled: true,
            created_ms: 0,
            updated_ms: 0,
            operator: "tester".into(),
            unique_id: String::new(),
            mavlink_addr: String::new(),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            mavlink_version: 2,
            mavlink_sysid: 1,
            mavlink_compid: 1,
            adapter_id: String::new(),
            drone_tag: String::new(),
        }
    }

    #[test]
    fn unique_id_must_be_hex() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.unique_id = "zzz".into();
        assert!(upsert_profile(&state, p, "tester").is_err());
    }

    #[test]
    fn unique_id_is_normalized_to_lowercase() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.unique_id = "ABCDEF".into();
        let rec = upsert_profile(&state, p, "tester").expect("upsert");
        assert_eq!(rec.unique_id, "abcdef");
    }

    #[test]
    fn mavlink_version_must_be_1_or_2() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.mavlink_version = 3;
        assert!(upsert_profile(&state, p, "tester").is_err());
    }

    #[test]
    fn mavlink1_cannot_have_signing_secret() {
        let state = state_with_master();
        let mut p = profile();
        p.mavlink_version = 1;
        p.signing_secret = "aabb".into();
        assert!(upsert_profile(&state, p, "tester").is_err());
    }

    #[test]
    fn sysid_and_compid_must_be_in_range() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.mavlink_sysid = 0;
        assert!(upsert_profile(&state, p, "tester").is_err());
        let mut p = profile();
        p.mavlink_compid = 300;
        assert!(upsert_profile(&state, p, "tester").is_err());
    }

    #[test]
    fn signing_secret_is_stored_encrypted() {
        let state = state_with_master();
        let mut p = profile();
        p.signing_secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into();
        let rec = upsert_profile(&state, p, "tester").expect("upsert");
        assert!(rec.signing_secret.starts_with("v1:"));
        assert!(!rec.signing_secret.contains("0011223344556677"));
    }

    #[test]
    fn drone_tag_is_generated_and_stable_across_updates() {
        let state = DroneState::new(DroneConfig::default());
        let first = upsert_profile(&state, profile(), "tester").unwrap();
        assert!(first.drone_tag.starts_with("drone-"));

        let mut changed = profile();
        changed.drone_tag = "drone-manual".into();
        let second = upsert_profile(&state, changed, "tester").unwrap();
        assert_eq!(second.drone_tag, first.drone_tag);
    }

    #[test]
    fn duplicate_drone_tag_is_rejected() {
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.device_id = "drone-a".into();
        first.drone_tag = "drone-fixed".into();
        upsert_profile(&state, first, "tester").unwrap();

        let mut second = profile();
        second.device_id = "drone-b".into();
        second.drone_tag = "drone-fixed".into();
        assert!(upsert_profile(&state, second, "tester").is_err());
    }

    #[test]
    fn drone_tag_is_kept_when_resubmitting_own_tag() {
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.device_id = "drone-a".into();
        first.drone_tag = "drone-fixed".into();
        let created = upsert_profile(&state, first, "tester").unwrap();

        let mut again = profile();
        again.device_id = "drone-a".into();
        again.drone_tag = "drone-fixed".into();
        let updated = upsert_profile(&state, again, "tester").expect("resubmit own tag");
        assert_eq!(updated.drone_tag, created.drone_tag);
        assert_eq!(updated.drone_tag, "drone-fixed");
    }

    #[test]
    fn padded_device_id_update_keeps_generated_drone_tag() {
        let state = DroneState::new(DroneConfig::default());
        let first = upsert_profile(&state, profile(), "tester").unwrap();

        let mut padded = profile();
        padded.device_id = "  drone-001  ".into();
        let second = upsert_profile(&state, padded, "tester").expect("padded device_id update");
        assert_eq!(second.drone_tag, first.drone_tag);
    }

    #[test]
    fn signing_secret_requires_master_key() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.signing_secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into();
        assert!(upsert_profile(&state, p, "tester").is_err());
    }
}
