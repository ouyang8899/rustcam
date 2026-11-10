//! 产品管理面：无人机档案、航线、围栏，以及航线执行编排。
//!
//! 档案与航线属于业务数据（可先于设备上线创建）；航线执行复用既有指令状态机，
//! 顺序下发 `goto`，失败即中止并写审计。

use rustcam_drone_core::command::{is_terminal_status, status};
use rustcam_drone_core::telemetry::Telemetry;
use rustcam_drone_proto::common as proto;
use rustcam_drone_proto::control::{
    Adapter, DroneProfile, Geofence as ProtoFence, Route as ProtoRoute, RouteWaypoint,
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
    drone_tag: &str,
    category: &str,
    action: &str,
    operator: &str,
    result: &str,
    detail: &str,
) {
    state.push_audit(AuditRecord {
        ts_ms: now_ms(),
        drone_tag: drone_tag.to_string(),
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
    profiles.sort_by(|a, b| a.drone_tag.cmp(&b.drone_tag));
    profiles
}

/// 解析档案的唯一标识（平台主键）。
///
/// - 传入为空 → 生成 `drone-<uuid4>`；
/// - 传入已存在 → 沿用档案里的值（`drone_tag` 不可变）；
/// - 传入新的 → 直接采用，但不得与其它档案重名（主键同形，天然唯一）。
fn resolve_drone_tag(state: &DroneState, incoming: &str) -> Result<String, String> {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return Ok(format!("drone-{}", uuid::Uuid::new_v4()));
    }
    if let Some(existing) = state
        .profiles
        .get(incoming)
        .map(|p| p.drone_tag.trim().to_string())
        .filter(|t| !t.is_empty())
    {
        return Ok(existing);
    }
    if let Some(other) = state
        .profiles
        .iter()
        .find(|e| e.key() != incoming && e.value().drone_tag.trim() == incoming)
    {
        return Err(format!("drone_tag `{incoming}` 已被 `{}` 使用", other.key()));
    }
    Ok(incoming.to_string())
}

pub fn profile_to_proto(p: &DroneProfileRecord) -> DroneProfile {
    DroneProfile {
        drone_tag: p.drone_tag.clone(),
        name: p.name.clone(),
        model: p.model.clone(),
        autopilot: p.autopilot.clone(),
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
    }
}

/// 取值策略：`0` 表示"未指定"——保留已有值，没有已有值时用默认值。
fn pick_or_keep(incoming: i32, existing: i32, default: i32) -> i32 {
    if incoming != 0 {
        return incoming;
    }
    if existing != 0 {
        existing
    } else {
        default
    }
}

pub fn upsert_profile(
    state: &DroneState,
    profile: DroneProfile,
    operator: &str,
) -> Result<DroneProfileRecord, String> {
    let autopilot = profile.autopilot.trim().to_ascii_lowercase();
    if !matches!(
        autopilot.as_str(),
        "" | "auto" | "px4" | "ardupilot" | "apm"
    ) {
        return Err(format!("unsupported autopilot `{autopilot}`"));
    }

    let now = now_ms();
    let drone_tag = resolve_drone_tag(state, &profile.drone_tag)?;
    let created_ms = state
        .profiles
        .get(&drone_tag)
        .map(|p| p.created_ms)
        .unwrap_or(now);

    // 机载链路身份字段。
    let unique_id = profile.unique_id.trim().to_ascii_lowercase();
    if !unique_id.is_empty() && !unique_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("unique_id must be lowercase hex".into());
    }
    // MAVLink 版本与寻址地址：0 表示"未指定"（查询接口不返回这些字段，
    // 客户端回传时会是 0）。未指定时保留已有值，新档案用默认值，
    // 不能因为 0 直接拒绝——否则档案无法编辑。
    let (prev_version, prev_sysid, prev_compid) = match state.profiles.get(&drone_tag) {
        Some(p) => (p.mavlink_version, p.mavlink_sysid, p.mavlink_compid),
        None => (0, 0, 0),
    };
    let mavlink_version = pick_or_keep(profile.mavlink_version, prev_version, 2);
    if mavlink_version != 1 && mavlink_version != 2 {
        return Err(format!("unsupported mavlink_version `{mavlink_version}`"));
    }
    let mavlink_sysid = pick_or_keep(profile.mavlink_sysid, prev_sysid, 1);
    if !(1..=255).contains(&mavlink_sysid) {
        return Err(format!("invalid mavlink_sysid `{mavlink_sysid}`"));
    }
    let mavlink_compid = pick_or_keep(profile.mavlink_compid, prev_compid, 1);
    if !(1..=255).contains(&mavlink_compid) {
        return Err(format!(
            "invalid mavlink_compid `{mavlink_compid}`"
        ));
    }
    // 链路绑定唯一：同一 adapter 下「协议族 + 端点 + 飞控系统号」三者组合唯一。
    // 同一端口可以承载多架机（按 sysid 区分），因此只看 sysid 会误判重复。
    let adapter_id = profile.adapter_id.trim().to_string();
    let mavlink_addr = profile.mavlink_addr.trim().to_string();
    let (link_family, link_endpoint) = match crate::link_identity::parse_endpoint(&mavlink_addr) {
        Some(ep) => (ep.family.as_str().to_string(), ep.endpoint),
        None => (String::new(), String::new()),
    };
    if !adapter_id.is_empty() && !link_family.is_empty() && mavlink_sysid > 0 {
        if let Some(other) = state.profiles.iter().find(|e| {
            e.key() != &drone_tag
                && e.value().adapter_id.trim() == adapter_id.as_str()
                && e.value().link_family == link_family
                && e.value().link_endpoint == link_endpoint
                && e.value().mavlink_sysid == mavlink_sysid
        }) {
            return Err(format!(
                "链路身份（协议族+端点+sysid）已被 `{}` 占用",
                other.key()
            ));
        }
    }
    // 签名密钥以密文入库；没有主密钥时拒绝保存明文密钥。
    let secret_plain = profile.signing_secret.trim().to_string();
    if !secret_plain.is_empty() && mavlink_version == 1 {
        return Err("MAVLink 1 has no signature field: clear signing_secret".into());
    }
    let signing_secret = if secret_plain.is_empty() {
        // 留空表示"保持原密钥"：密钥本体不外传（接口只回"是否已配置"），
        // 前端回传的密钥必然为空。直接置空会把已配置的密钥抹掉，
        // adapter 下发的签名校验随之失效。
        state
            .profiles
            .get(&drone_tag)
            .map(|p| p.signing_secret.clone())
            .unwrap_or_default()
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

    let record = DroneProfileRecord {
        drone_tag: drone_tag.clone(),
        name: if profile.name.trim().is_empty() {
            drone_tag.clone()
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
        video_stream: profile.video_stream.trim().to_string(),
        notes: profile.notes.trim().to_string(),
        enabled: profile.enabled,
        created_ms,
        updated_ms: now,
        unique_id,
        mavlink_addr,
        signing_secret,
        signing_allow_unsigned: profile.signing_allow_unsigned,
        mavlink_version,
        mavlink_sysid,
        mavlink_compid,
        adapter_id,
        link_family,
        link_endpoint,
    };
    state
        .profiles
        .insert(record.drone_tag.clone(), record.clone());
    audit(
        state,
        &drone_tag,
        "profile",
        "upsert",
        operator,
        "ok",
        &format!("name={}", record.name),
    );
    Ok(record)
}

/// 给未指定链路的档案自动分配一个空位（端口 + 飞控系统号）。
///
/// 端口写在 adapter 上（端口段），系统号按端口内空位分配：
/// 一个端口可承载 255 架（按 sysid 区分），因此接 200 架只需登记 200 次身份，
/// 端口不用重复写。
pub async fn assign_link_slot(
    state: &DroneState,
    profile: &mut DroneProfile,
) -> Result<(), String> {
    if !profile.mavlink_addr.trim().is_empty() {
        // 显式给了地址就按它走（端口共享由 sysid 区分）。
        return Ok(());
    }
    let adapter_id = profile.adapter_id.trim().to_string();
    if adapter_id.is_empty() {
        return Err("mavlink_addr 与 adapter_id 至少要给出其一".into());
    }
    // 更新已登记的机：地址留空表示沿用原地址，不重新分配空位。
    let own_tag = profile.drone_tag.trim().to_string();
    if !own_tag.is_empty() {
        if let Some(prev) = state.profiles.get(&own_tag) {
            if prev.adapter_id.trim() == adapter_id.as_str()
                && !prev.mavlink_addr.trim().is_empty()
            {
                profile.mavlink_addr = prev.mavlink_addr.clone();
                profile.mavlink_sysid = prev.mavlink_sysid;
                return Ok(());
            }
        }
    }
    let Some(db) = state.db.clone() else {
        return Err("database is not configured".into());
    };
    let adapter = match db
        .load_adapter(&adapter_id)
        .await
        .map_err(|e| format!("load adapter `{adapter_id}` failed: {e}"))?
    {
        Some(row) => row,
        None => {
            // 允许先登记无人机、后接 adapter：缺失时按默认值建行，
            // adapter 首次注册时再回写它的在线状态。
            let row = crate::infra::db::AdapterRow {
                adapter_id: adapter_id.clone(),
                name: String::new(),
                protocol: "mavlink".into(),
                bind_addr: "0.0.0.0".into(),
                port_start: 14540,
                port_end: 14560,
                autopilot: "auto".into(),
                signing_secret: String::new(),
                signing_allow_unsigned: true,
                reconnect_min_secs: 1,
                reconnect_max_secs: 30,
                online: false,
                enabled: true,
            };
            db.upsert_adapter(&row)
                .await
                .map_err(|e| format!("create adapter `{adapter_id}` failed: {e}"))?;
            tracing::info!(adapter = %adapter_id, "[Admin] created default adapter row");
            row
        }
    };
    // 停用（软删除）的 adapter 不再接纳新无人机；已绑定的机沿用原端点，不受影响。
    ensure_adapter_assignable(&adapter)?;

    // 已占用的 (端点, sysid) 组合：以 drone 表为准，内存档案补齐尚未落库的。
    // 更新同一架机时排除它自己的旧占用，否则每次保存都会换一个空位。
    let mut used: Vec<(String, i32)> = Vec::new();
    if let Ok(rows) = db.load_device_links(&adapter_id).await {
        for row in rows {
            if !own_tag.is_empty() && row.drone_tag == own_tag {
                continue;
            }
            let endpoint = if row.link_endpoint.is_empty() {
                crate::link_identity::parse_endpoint(&row.mavlink_addr)
                    .map(|e| e.endpoint)
                    .unwrap_or_default()
            } else {
                row.link_endpoint.clone()
            };
            if endpoint.is_empty() {
                continue;
            }
            let sysid = if row.fc_sysid > 0 {
                row.fc_sysid
            } else {
                row.mavlink_sysid
            };
            used.push((endpoint, sysid));
        }
    }
    for p in state.profiles.iter() {
        let p = p.value();
        if p.adapter_id.trim() != adapter_id.as_str() {
            continue;
        }
        if !own_tag.is_empty() && p.drone_tag == own_tag {
            continue;
        }
        let endpoint = if p.link_endpoint.is_empty() {
            crate::link_identity::parse_endpoint(&p.mavlink_addr)
                .map(|e| e.endpoint)
                .unwrap_or_default()
        } else {
            p.link_endpoint.clone()
        };
        if endpoint.is_empty() {
            continue;
        }
        used.push((endpoint, p.mavlink_sysid));
    }

    let port_start = adapter.port_start.max(1);
    let port_end = adapter.port_end.max(port_start);
    // 操作员指定了 sysid 就按它找空端口；没指定则取最小空位。
    let preferred_sysid = if profile.mavlink_sysid > 0 {
        Some(profile.mavlink_sysid)
    } else {
        None
    };
    let Some((port, sysid)) = find_free_slot(&used, port_start, port_end, preferred_sysid) else {
        return Err(format!(
            "adapter `{adapter_id}` 的端口段 {port_start}-{port_end} 已用满"
        ));
    };
    profile.mavlink_addr = format!("udpin:{}:{port}", adapter.bind_addr);
    profile.mavlink_sysid = sysid;
    Ok(())
}

/// 在端口段内找一个空闲的 (端口, 飞控系统号)。
///
/// 每个端口按 sysid 区分，最多承载 255 架（sysid 1..=255）；
/// 因此接 200 架只需登记 200 次身份，端口由平台自动分配。
fn find_free_slot(
    used: &[(String, i32)],
    port_start: i32,
    port_end: i32,
    preferred_sysid: Option<i32>,
) -> Option<(i32, i32)> {
    let is_free = |port: i32, sysid: i32| {
        let endpoint = port.to_string();
        !used.iter().any(|(e, s)| e == &endpoint && *s == sysid)
    };
    match preferred_sysid {
        // 指定了系统号：只找能放下该号的端口。
        Some(sysid) if (1..=255).contains(&sysid) => (port_start..=port_end)
            .find(|p| is_free(*p, sysid))
            .map(|p| (p, sysid)),
        _ => {
            for port in port_start..=port_end {
                for sysid in 1..=255 {
                    if is_free(port, sysid) {
                        return Some((port, sysid));
                    }
                }
            }
            None
        }
    }
}

// ---------------------------------------------------------------------------
// 机载链路实体（adapter）
// ---------------------------------------------------------------------------

/// adapter 标识允许的字符：字母、数字、点、下划线、连字符。
fn adapter_id_is_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// 校验并规范化 adapter 配置（协议、监听地址、端口段、重连与签名策略）。
///
/// `0` / 空串表示"未指定"：按默认值补齐，避免客户端表单回传 0 时被拒。
pub fn adapter_from_proto(adapter: &Adapter) -> Result<crate::infra::db::AdapterRow, String> {
    let adapter_id = adapter.adapter_id.trim().to_string();
    if !adapter_id_is_valid(&adapter_id) {
        return Err(format!(
            "invalid adapter_id `{adapter_id}`：只允许字母、数字、点、下划线与连字符"
        ));
    }
    let protocol = adapter.protocol.trim().to_ascii_lowercase();
    let protocol = if protocol.is_empty() {
        "mavlink".to_string()
    } else {
        protocol
    };
    let autopilot = adapter.autopilot.trim().to_ascii_lowercase();
    let autopilot = match autopilot.as_str() {
        "" => "auto".to_string(),
        "apm" => "ardupilot".to_string(),
        "auto" | "px4" | "ardupilot" => autopilot,
        other => return Err(format!("unsupported autopilot `{other}`")),
    };
    let bind_addr = adapter.bind_addr.trim().to_string();
    let bind_addr = if bind_addr.is_empty() {
        "0.0.0.0".to_string()
    } else {
        bind_addr
    };
    let port_start = if adapter.port_start <= 0 {
        14540
    } else {
        adapter.port_start
    };
    let port_end = if adapter.port_end <= 0 {
        14560
    } else {
        adapter.port_end
    };
    if !(1..=65535).contains(&port_start) || !(1..=65535).contains(&port_end) {
        return Err(format!(
            "invalid port range {port_start}-{port_end}：端口必须在 1-65535 之间"
        ));
    }
    if port_start > port_end {
        return Err(format!(
            "invalid port range {port_start}-{port_end}：起始端口不能大于结束端口"
        ));
    }
    let reconnect_min_secs = if adapter.reconnect_min_secs <= 0 {
        1
    } else {
        adapter.reconnect_min_secs
    };
    let reconnect_max_secs = if adapter.reconnect_max_secs <= 0 {
        30
    } else {
        adapter.reconnect_max_secs
    };
    if reconnect_min_secs > reconnect_max_secs {
        return Err(format!(
            "invalid reconnect range {reconnect_min_secs}-{reconnect_max_secs}：下限不能大于上限"
        ));
    }
    let signing_secret = adapter.signing_secret.trim().to_string();
    if !signing_secret.is_empty()
        && (!signing_secret.chars().all(|c| c.is_ascii_hexdigit())
            || !signing_secret.len().is_multiple_of(2))
    {
        return Err("signing_secret must be an even-length hex string".into());
    }
    Ok(crate::infra::db::AdapterRow {
        adapter_id,
        name: adapter.name.trim().to_string(),
        protocol,
        bind_addr,
        port_start,
        port_end,
        autopilot,
        signing_secret,
        signing_allow_unsigned: adapter.signing_allow_unsigned,
        reconnect_min_secs,
        reconnect_max_secs,
        online: false,
        enabled: adapter.enabled,
    })
}

/// 合并 adapter 配置：编辑已有行时保留启停状态与在线状态。
///
/// 启停只能走专用接口（软删除），配置编辑不能顺手把停用的 adapter 复活；
/// 在线状态由适配器注册维护，配置编辑更不能覆盖。
pub fn merge_adapter(
    existing: Option<&crate::infra::db::AdapterRow>,
    incoming: &Adapter,
) -> Result<crate::infra::db::AdapterRow, String> {
    let mut row = adapter_from_proto(incoming)?;
    match existing {
        Some(prev) => {
            // 接口回包会抹掉密钥；编辑时留空表示保持原密钥。
            if row.signing_secret.trim().is_empty() {
                row.signing_secret = prev.signing_secret.clone();
            }
            row.enabled = prev.enabled;
            row.online = prev.online;
        }
        None => {
            row.enabled = true;
        }
    }
    Ok(row)
}

/// adapter 行转回协议消息。
pub fn adapter_to_proto(row: &crate::infra::db::AdapterRow) -> Adapter {
    Adapter {
        adapter_id: row.adapter_id.clone(),
        name: row.name.clone(),
        protocol: row.protocol.clone(),
        bind_addr: row.bind_addr.clone(),
        port_start: row.port_start,
        port_end: row.port_end,
        autopilot: row.autopilot.clone(),
        signing_secret: row.signing_secret.clone(),
        signing_allow_unsigned: row.signing_allow_unsigned,
        reconnect_min_secs: row.reconnect_min_secs,
        reconnect_max_secs: row.reconnect_max_secs,
        online: row.online,
        enabled: row.enabled,
        operator: String::new(),
    }
}

/// 停用（软删除）的 adapter 不再接纳新无人机；已绑定的无人机继续运行。
pub fn ensure_adapter_assignable(row: &crate::infra::db::AdapterRow) -> Result<(), String> {
    if row.enabled {
        Ok(())
    } else {
        Err(format!(
            "adapter `{}` 已停用，不能绑定新无人机",
            row.adapter_id
        ))
    }
}

/// 列出 adapter 配置。
pub async fn list_adapters(
    state: &DroneState,
    include_disabled: bool,
) -> Result<Vec<crate::infra::db::AdapterRow>, String> {
    let Some(db) = state.db.clone() else {
        return Err("database is not configured".into());
    };
    let rows = db
        .load_adapters(include_disabled)
        .await
        .map_err(|e| format!("adapter 列表读取失败：{e}"))?;
    Ok(rows)
}

/// 读取单个 adapter 配置。
pub async fn get_adapter(
    state: &DroneState,
    adapter_id: &str,
) -> Result<Option<crate::infra::db::AdapterRow>, String> {
    let Some(db) = state.db.clone() else {
        return Err("database is not configured".into());
    };
    db.load_adapter(adapter_id.trim())
        .await
        .map_err(|e| format!("adapter 读取失败：{e}"))
}

/// 新建或更新 adapter 配置。
pub async fn upsert_adapter(
    state: &DroneState,
    adapter: Adapter,
    operator: &str,
) -> Result<crate::infra::db::AdapterRow, String> {
    let adapter_id = adapter.adapter_id.trim().to_string();
    let db = state
        .db
        .clone()
        .ok_or_else(|| "database is not configured".to_string())?;
    let existing = db
        .load_adapter(&adapter_id)
        .await
        .map_err(|e| format!("adapter 读取失败：{e}"))?;
    let row = merge_adapter(existing.as_ref(), &adapter)?;
    db.upsert_adapter(&row)
        .await
        .map_err(|e| format!("adapter 保存失败：{e}"))?;
    audit(
        state,
        "",
        "adapter",
        "upsert",
        operator,
        "ok",
        &format!("adapter_id={} protocol={}", row.adapter_id, row.protocol),
    );
    Ok(row)
}

/// 停用（软删除）或恢复 adapter。
///
/// 停用只改启停标记：配置行与已绑定关系都保留，便于审计与回滚。
pub async fn set_adapter_enabled(
    state: &DroneState,
    adapter_id: &str,
    enabled: bool,
    operator: &str,
) -> Result<crate::infra::db::AdapterRow, String> {
    let adapter_id = adapter_id.trim().to_string();
    let db = state
        .db
        .clone()
        .ok_or_else(|| "database is not configured".to_string())?;
    let mut row = db
        .load_adapter(&adapter_id)
        .await
        .map_err(|e| format!("adapter 读取失败：{e}"))?
        .ok_or_else(|| format!("adapter `{adapter_id}` 不存在"))?;
    row.enabled = enabled;
    db.upsert_adapter(&row)
        .await
        .map_err(|e| format!("adapter 保存失败：{e}"))?;
    audit(
        state,
        "",
        "adapter",
        if enabled { "enable" } else { "disable" },
        operator,
        "ok",
        &format!("adapter_id={adapter_id}"),
    );
    Ok(row)
}

/// 把机载链路参数写入 `drone_device` 表。
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
        drone_tag: record.drone_tag.clone(),
        unique_id: record.unique_id.clone(),
        mavlink_addr: record.mavlink_addr.clone(),
        signing_secret: record.signing_secret.clone(),
        signing_allow_unsigned: record.signing_allow_unsigned,
        mavlink_version: record.mavlink_version,
        mavlink_sysid: record.mavlink_sysid,
        mavlink_compid: record.mavlink_compid,
        adapter_id: record.adapter_id.clone(),
        link_family: record.link_family.clone(),
        link_endpoint: record.link_endpoint.clone(),
        fc_sysid: record.mavlink_sysid,
    };
    db.upsert_device_link(&row)
        .await
        .map_err(|e| format!("persist link params failed: {e}"))
}

pub async fn delete_profile(
    state: &DroneState,
    drone_tag: &str,
    operator: &str,
) -> Result<bool, String> {
    let existed = state.profiles.remove(drone_tag).is_some();
    if existed {
        // 多对多航线：解绑该无人机；若它是唯一适用机则整条航线删除。
        let affected: Vec<(String, RouteRecord)> = state
            .routes
            .iter()
            .filter(|r| r.value().drone_tags.iter().any(|id| id == drone_tag))
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect();
        for (id, mut route) in affected {
            route.drone_tags.retain(|x| x != drone_tag);
            if route.drone_tags.is_empty() {
                // 关联清空的航线整体删除：否则会退化成"适用于全部无人机"的模板。
                if let Some(db) = state.db.clone() {
                    db.delete_route(&id)
                        .await
                        .map_err(|e| format!("航线删除入库失败：{e}"))?;
                }
                state.routes.remove(&id);
                continue;
            }
            if let Some(db) = state.db.clone() {
                db.upsert_route(&route)
                    .await
                    .map_err(|e| format!("航线入库失败：{e}"))?;
            }
            state.routes.insert(id, route);
        }
        audit(state, drone_tag, "profile", "delete", operator, "ok", "");
    }
    Ok(existed)
}

// ---------------------------------------------------------------------------
// 航线
// ---------------------------------------------------------------------------

pub fn list_routes(state: &DroneState, drone_tag: &str) -> Vec<RouteRecord> {
    let mut routes: Vec<_> = state
        .routes
        .iter()
        .filter(|r| {
            let v = r.value();
            if drone_tag.is_empty() {
                return true;
            }
            // 关联表为空表示模板航线：对所有无人机可用。
            v.drone_tags.is_empty() || v.drone_tags.iter().any(|id| id == drone_tag)
        })
        .map(|e| e.value().clone())
        .collect();
    routes.sort_by(|a, b| a.updated_ms.cmp(&b.updated_ms).reverse());
    routes
}

pub fn route_to_proto(r: &RouteRecord) -> ProtoRoute {
    ProtoRoute {
        id: r.id.clone(),
        name: r.name.clone(),
        // 单值字段是关联列表的派生值（仅一架时有值），供旧客户端使用。
        drone_tag: r.sole_drone_tag().to_string(),
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
        drone_tags: r.drone_tags.clone(),
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
    let mut drone_tags: Vec<String> = route
        .drone_tags
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // 兼容旧客户端只传 drone_tag。
    if drone_tags.is_empty() && !route.drone_tag.trim().is_empty() {
        drone_tags.push(route.drone_tag.trim().to_string());
    }
    drone_tags.sort();
    drone_tags.dedup();
    let end_action = if route.end_action.trim().is_empty() {
        normalize_end_action(&state.config.route_default_end_action).to_string()
    } else {
        normalize_end_action(&route.end_action).to_string()
    };
    let record = RouteRecord {
        id: id.clone(),
        name: route.name.trim().to_string(),
        drone_tags,
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
        record.sole_drone_tag(),
        "route",
        "upsert",
        operator,
        "ok",
        &format!(
            "route_id={id} waypoints={} devices={}",
            record.waypoints.len(),
            record.drone_tags.len()
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
    drone_tag: &str,
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
    let target = drone_tag.trim();
    if target.is_empty() {
        return Err("一次只允许执行一架无人机，请指定执行目标；多架请逐架执行".into());
    }
    let target = target.to_string();
    // 关联表为空表示模板航线：对所有无人机可用。
    let applies = route.drone_tags.is_empty() || route.drone_tags.iter().any(|id| id == &target);
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
    drone_tag: String,
}

impl Drop for MissionReservation {
    fn drop(&mut self) {
        self.state.active_missions.remove(&self.drone_tag);
    }
}

/// 多机同空域互斥：同一架无人机只能有一个任务；不同无人机的航点
/// 水平 <50m 且高差 <15m 视为冲突，直接拒绝启动，避免空中相撞。
fn reserve_mission(
    state: &DroneState,
    drone_tag: &str,
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
    if let Some(entry) = state.active_missions.get(drone_tag) {
        return Err(format!(
            "无人机 `{drone_tag}` 已有任务（航线 `{}`）在执行",
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
        drone_tag.to_string(),
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
        drone_tag: drone_tag.to_string(),
    })
}

fn latest_telemetry(state: &DroneState, drone_tag: &str) -> Option<Telemetry> {
    state
        .devices
        .get(drone_tag)
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

fn build_takeoff(route: &RouteRecord, alt: f64, operator: &str, drone_tag: &str) -> proto::Command {
    let mut params = std::collections::HashMap::new();
    params.insert("alt".to_string(), alt.max(5.0).to_string());
    proto::Command {
        cmd_id: format!("{}-takeoff-{}", route.id, uuid::Uuid::new_v4().simple()),
        drone_tag: drone_tag.to_string(),
        r#type: "takeoff".into(),
        params,
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

fn build_rtl(route: &RouteRecord, operator: &str, drone_tag: &str) -> proto::Command {
    proto::Command {
        cmd_id: format!("{}-rtl-{}", route.id, uuid::Uuid::new_v4().simple()),
        drone_tag: drone_tag.to_string(),
        r#type: "rtl".into(),
        params: std::collections::HashMap::new(),
        deadline_ms: now_ms() + 60_000,
        operator: operator.to_string(),
    }
}

fn build_land(route: &RouteRecord, operator: &str, drone_tag: &str) -> proto::Command {
    proto::Command {
        cmd_id: format!("{}-land-{}", route.id, uuid::Uuid::new_v4().simple()),
        drone_tag: drone_tag.to_string(),
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
    drone_tag: &str,
) -> proto::Command {
    let w = &route.waypoints[index];
    let mut params = std::collections::HashMap::new();
    params.insert("lat".to_string(), w.lat.to_string());
    params.insert("lon".to_string(), w.lon.to_string());
    params.insert("alt".to_string(), w.alt.to_string());
    proto::Command {
        cmd_id: format!("{}-{}", route.id, uuid::Uuid::new_v4().simple()),
        drone_tag: drone_tag.to_string(),
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
    drone_tag: &str,
    reason: &str,
) {
    let mut cmd = build_rtl(route, "system", drone_tag);
    cmd.cmd_id = format!("{}-safety-rtl-{}", route.id, uuid::Uuid::new_v4().simple());
    let reply = commands::submit_safety(state, cmd).await;
    let result = if mission_started(&reply) {
        "ok"
    } else {
        reply.status.as_str()
    };
    audit(
        state,
        &drone_tag,
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
    drone_tag: String,
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
            &drone_tag,
            control::CONTROL_FLIGHT,
            &operator,
        )
        .await
        {
            // 区分“被接管”和“无人持有”：被接管时交给新持有人；无人持有时安全返航。
            if let Some(lease) =
                control::get(&state, &drone_tag, control::CONTROL_FLIGHT).await
            {
                audit(
                    &state,
                    &drone_tag,
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
                &drone_tag,
                "route",
                "abort",
                &operator,
                "control_expired",
                &format!("route_id={} waypoint={} {e}", route.id, index + 1),
            );
            send_safety_rtl(&state, &route, &drone_tag, "control_expired").await;
            return;
        }

        // 未解锁时由服务端先发起飞指令，等离地后再开始航点。
        if index == 0
            && wait_takeoff
            && !wait_airborne(
                &state,
                &drone_tag,
                takeoff_alt,
                std::time::Duration::from_secs(60),
            )
            .await
        {
            audit(
                &state,
                &drone_tag,
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
                    &drone_tag,
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
                &drone_tag,
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
            &drone_tag,
            waypoint.lat,
            waypoint.lon,
            std::time::Duration::from_secs(WAYPOINT_REACH_TIMEOUT_SECS),
        )
        .await
        {
            audit(
                &state,
                &drone_tag,
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
            &drone_tag,
            "route",
            "complete",
            &operator,
            "ok",
            &format!("route_id={} end_action=hold", route.id),
        );
        return;
    }

    if let Err(e) =
        control::ensure_holder(&state, &drone_tag, control::CONTROL_FLIGHT, &operator).await
    {
        if control::get(&state, &drone_tag, control::CONTROL_FLIGHT)
            .await
            .is_some()
        {
            audit(
                &state,
                &drone_tag,
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
                &drone_tag,
                "route",
                "complete",
                &operator,
                "control_expired",
                &format!("route_id={} safety_rtl: {e}", route.id),
            );
            send_safety_rtl(&state, &route, &drone_tag, "control_expired_before_end_action").await;
        }
        return;
    }
    let cmd = if end_action == "rtl" {
        build_rtl(&route, &operator, &drone_tag)
    } else {
        build_land(&route, &operator, &drone_tag)
    };
    let reply = commands::submit(&state, cmd).await;
    let result = if mission_started(&reply) {
        "ok"
    } else {
        reply.status.as_str()
    };
    audit(
        &state,
        &drone_tag,
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
    drone_tag: &str,
    target_alt: f64,
    timeout: std::time::Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    let need = (target_alt * 0.7).clamp(2.0, 10.0);
    loop {
        if let Some(t) = latest_telemetry(state, drone_tag) {
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
    drone_tag: &str,
    lat: f64,
    lon: f64,
    timeout: std::time::Duration,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(entry) = state.devices.get(drone_tag) {
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
        // 单值字段是关联列表的派生值（仅一架时有值），供旧客户端使用。
        drone_tag: f.sole_drone_tag().to_string(),
        drone_tags: f.drone_tags.clone(),
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
    let mut drone_tags: Vec<String> = fence
        .drone_tags
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    // 兼容旧客户端只传 drone_tag 的情况。
    if drone_tags.is_empty() && !fence.drone_tag.trim().is_empty() {
        drone_tags.push(fence.drone_tag.trim().to_string());
    }
    drone_tags.sort();
    drone_tags.dedup();
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
        drone_tags,
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
        record.sole_drone_tag(),
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
    use crate::infra::db::AdapterRow;
    use rustcam_drone_proto::control::Adapter;

    const MASTER: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn state_with_master() -> DroneState {
        let mut cfg = DroneConfig::default();
        cfg.signing_master_key = Some(MASTER.to_string());
        DroneState::new(cfg)
    }

    /// adapter 的落库行夹具；`enabled` / `online` 由各用例单独覆盖。
    fn adapter_row_fixture() -> AdapterRow {
        AdapterRow {
            adapter_id: "adapter-1".into(),
            name: String::new(),
            protocol: "mavlink".into(),
            bind_addr: "0.0.0.0".into(),
            port_start: 14540,
            port_end: 14560,
            autopilot: "auto".into(),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            reconnect_min_secs: 1,
            reconnect_max_secs: 30,
            online: false,
            enabled: true,
        }
    }

    fn profile() -> DroneProfile {
        DroneProfile {
            drone_tag: "drone-001".into(),
            name: String::new(),
            model: String::new(),
            autopilot: "auto".into(),
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
        }
    }

    #[test]
    fn zero_sysid_and_compid_fall_back_to_defaults() {
        // 客户端（前端表单 / 查询接口）未带这三个字段时会传 0：
        // 不能因此拒绝保存（否则档案变成只读）。
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.mavlink_sysid = 0;
        p.mavlink_compid = 0;
        p.mavlink_version = 0;
        let rec = upsert_profile(&state, p, "tester").expect("应当保存成功");
        assert_eq!(rec.mavlink_sysid, 1);
        assert_eq!(rec.mavlink_compid, 1);
        assert_eq!(rec.mavlink_version, 2);
    }

    #[test]
    fn zero_sysid_keeps_previously_configured_value() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.mavlink_sysid = 42;
        p.mavlink_compid = 7;
        upsert_profile(&state, p, "tester").expect("保存");

        let mut p2 = profile();
        p2.mavlink_sysid = 0;
        p2.mavlink_compid = 0;
        let rec = upsert_profile(&state, p2, "tester").expect("应当保存成功");
        assert_eq!(rec.mavlink_sysid, 42, "未指定时不应覆盖已配置值");
        assert_eq!(rec.mavlink_compid, 7);
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
        // 0 = 未指定（查询接口不返回该字段，客户端回传 0）→ 取默认值，不报错。
        let mut p = profile();
        p.mavlink_sysid = 0;
        let rec = upsert_profile(&state, p, "tester").expect("0 视为未指定");
        assert_eq!(rec.mavlink_sysid, 1);
        // 越界仍然拒绝。
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
    fn blank_signing_secret_keeps_previously_configured_secret() {
        // 签名密钥不外传（接口只回"是否已配置"），前端回传的密钥必然为空：
        // 留空必须表示"保持原密钥"，否则每次编辑档案都会把已配置的密钥抹掉，
        // adapter 下发的签名校验随之失效。
        let state = state_with_master();
        let mut p = profile();
        p.signing_secret =
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into();
        let sealed = upsert_profile(&state, p, "tester")
            .expect("upsert")
            .signing_secret;
        assert!(sealed.starts_with("v1:"));

        let mut p2 = profile();
        p2.signing_secret = String::new();
        let rec = upsert_profile(&state, p2, "tester").expect("upsert");
        assert_eq!(rec.signing_secret, sealed, "留空应保持原密钥，不能被抹掉");
    }

    #[test]
    fn blank_signing_secret_on_new_profile_stays_empty() {
        let state = state_with_master();
        let rec = upsert_profile(&state, profile(), "tester").expect("upsert");
        assert!(rec.signing_secret.is_empty());
    }

    #[test]
    fn drone_tag_is_generated_when_absent() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.drone_tag = String::new();
        let rec = upsert_profile(&state, p, "tester").expect("应当保存成功");
        assert!(rec.drone_tag.starts_with("drone-"));
    }

    #[test]
    fn duplicate_link_identity_is_rejected() {
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.drone_tag = "drone-a".into();
        first.adapter_id = "adapter-1".into();
        first.mavlink_addr = "udpin:0.0.0.0:14540".into();
        first.mavlink_sysid = 12;
        upsert_profile(&state, first, "tester").expect("首次应当成功");

        // 同 adapter、同端点、同 sysid → 同一架飞控，重复。
        let mut second = profile();
        second.drone_tag = "drone-b".into();
        second.adapter_id = "adapter-1".into();
        second.mavlink_addr = "udpin:0.0.0.0:14540".into();
        second.mavlink_sysid = 12;
        assert!(
            upsert_profile(&state, second, "tester").is_err(),
            "同 adapter 同端点同 sysid 不得重复"
        );
    }

    #[test]
    fn same_port_different_sysid_is_allowed() {
        // 一个端口可承载多架机（按 sysid 区分），因此同端口不同 sysid 合法。
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.drone_tag = "drone-a".into();
        first.adapter_id = "adapter-1".into();
        first.mavlink_addr = "udpin:0.0.0.0:14540".into();
        first.mavlink_sysid = 12;
        upsert_profile(&state, first, "tester").expect("首次应当成功");

        let mut second = profile();
        second.drone_tag = "drone-b".into();
        second.adapter_id = "adapter-1".into();
        second.mavlink_addr = "udpin:0.0.0.0:14540".into();
        second.mavlink_sysid = 13;
        let rec = upsert_profile(&state, second, "tester").expect("同端口不同 sysid 应当允许");
        assert_eq!(rec.mavlink_sysid, 13);
    }

    #[test]
    fn same_sysid_different_port_is_allowed() {
        // 不同端口属于不同端点，sysid 可以相同。
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.drone_tag = "drone-a".into();
        first.adapter_id = "adapter-1".into();
        first.mavlink_addr = "udpin:0.0.0.0:14540".into();
        first.mavlink_sysid = 12;
        upsert_profile(&state, first, "tester").expect("首次应当成功");

        let mut second = profile();
        second.drone_tag = "drone-b".into();
        second.adapter_id = "adapter-1".into();
        second.mavlink_addr = "udpin:0.0.0.0:14541".into();
        second.mavlink_sysid = 12;
        upsert_profile(&state, second, "tester").expect("不同端点同 sysid 应当允许");
    }

    #[test]
    fn free_slot_prefers_first_port_and_lowest_sysid() {
        let slot = find_free_slot(&[], 14540, 14560, None).expect("有空位");
        assert_eq!(slot, (14540, 1));
    }

    #[test]
    fn free_slot_skips_occupied_pairs() {
        // 14540 已被 sysid 1、2 占用 → 下一个空位是 sysid 3。
        let used = vec![("14540".to_string(), 1), ("14540".to_string(), 2)];
        assert_eq!(find_free_slot(&used, 14540, 14560, None), Some((14540, 3)));
    }

    #[test]
    fn free_slot_moves_to_next_port_when_current_is_full() {
        // 14540 的 1..=255 全满 → 落到 14541。
        let used: Vec<(String, i32)> = (1..=255).map(|s| ("14540".to_string(), s)).collect();
        assert_eq!(find_free_slot(&used, 14540, 14560, None), Some((14541, 1)));
    }

    #[test]
    fn free_slot_honours_explicit_sysid() {
        // 操作员指定 sysid = 42：即使 sysid 1 空闲，也应优先分配 42。
        assert_eq!(
            find_free_slot(&[], 14540, 14560, Some(42)),
            Some((14540, 42))
        );
        // 14540 的 42 已占用 → 换下一个端口。
        let used = vec![("14540".to_string(), 42)];
        assert_eq!(
            find_free_slot(&used, 14540, 14560, Some(42)),
            Some((14541, 42))
        );
    }

    #[test]
    fn free_slot_reports_exhaustion() {
        // 端口段只有 14540 且已满 → 没有空位。
        let used: Vec<(String, i32)> = (1..=255).map(|s| ("14540".to_string(), s)).collect();
        assert_eq!(find_free_slot(&used, 14540, 14540, None), None);
    }

    #[test]
    fn drone_tag_is_kept_when_resubmitting_own_tag() {
        let state = DroneState::new(DroneConfig::default());

        let mut first = profile();
        first.drone_tag = "drone-a".into();
        first.drone_tag = "drone-fixed".into();
        let created = upsert_profile(&state, first, "tester").unwrap();

        let mut again = profile();
        again.drone_tag = "drone-a".into();
        again.drone_tag = "drone-fixed".into();
        let updated = upsert_profile(&state, again, "tester").expect("resubmit own tag");
        assert_eq!(updated.drone_tag, created.drone_tag);
        assert_eq!(updated.drone_tag, "drone-fixed");
    }

    #[test]
    fn padded_drone_tag_update_keeps_generated_drone_tag() {
        let state = DroneState::new(DroneConfig::default());
        let first = upsert_profile(&state, profile(), "tester").unwrap();

        let mut padded = profile();
        padded.drone_tag = "  drone-001  ".into();
        let second = upsert_profile(&state, padded, "tester").expect("padded drone_tag update");
        assert_eq!(second.drone_tag, first.drone_tag);
    }

    #[test]
    fn signing_secret_requires_master_key() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.signing_secret = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into();
        assert!(upsert_profile(&state, p, "tester").is_err());
    }

    fn route_proto(drone_tags: &[&str]) -> ProtoRoute {
        ProtoRoute {
            id: "route-1".into(),
            name: "巡检".into(),
            drone_tag: String::new(),
            waypoints: vec![RouteWaypoint {
                lat: 31.2,
                lon: 121.4,
                alt: 30.0,
                speed: 8.0,
                loiter_sec: 0,
                action: "waypoint".into(),
            }],
            enabled: true,
            updated_ms: 0,
            operator: String::new(),
            end_action: String::new(),
            drone_tags: drone_tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn route_association_is_stored_as_drone_tag_list() {
        let state = DroneState::new(DroneConfig::default());
        let record = upsert_route(&state, route_proto(&[" drone-b ", "drone-a", "drone-a"]), "tester")
            .await
            .expect("upsert route");
        assert_eq!(record.drone_tags, vec!["drone-a", "drone-b"]);
        // 单值字段是关联列表的派生值。
        assert_eq!(record.sole_drone_tag(), "");
        assert_eq!(route_to_proto(&record).drone_tag, "");
        assert_eq!(route_to_proto(&record).drone_tags, vec!["drone-a", "drone-b"]);
    }

    #[tokio::test]
    async fn route_sole_drone_is_exposed_as_single_value() {
        let state = DroneState::new(DroneConfig::default());
        let record = upsert_route(&state, route_proto(&["drone-a"]), "tester")
            .await
            .expect("upsert route");
        assert_eq!(record.sole_drone_tag(), "drone-a");
        assert_eq!(route_to_proto(&record).drone_tag, "drone-a");
    }

    #[tokio::test]
    async fn route_falls_back_to_single_drone_tag_from_old_clients() {
        let state = DroneState::new(DroneConfig::default());
        let mut proto = route_proto(&[]);
        proto.drone_tag = "drone-a".into();
        let record = upsert_route(&state, proto, "tester")
            .await
            .expect("upsert route");
        assert_eq!(record.drone_tags, vec!["drone-a"]);
    }

    #[tokio::test]
    async fn template_route_applies_to_every_drone() {
        let state = DroneState::new(DroneConfig::default());
        upsert_route(&state, route_proto(&[]), "tester")
            .await
            .expect("upsert route");
        assert_eq!(list_routes(&state, "drone-a").len(), 1);
        assert_eq!(list_routes(&state, "").len(), 1);
    }

    #[tokio::test]
    async fn route_listing_filters_by_drone_tag() {
        let state = DroneState::new(DroneConfig::default());
        upsert_route(&state, route_proto(&["drone-a"]), "tester")
            .await
            .expect("upsert route");
        assert_eq!(list_routes(&state, "drone-a").len(), 1);
        assert_eq!(list_routes(&state, "drone-b").len(), 0);
    }

    #[tokio::test]
    async fn deleting_profile_unbinds_route_and_removes_orphan_route() {
        let state = DroneState::new(DroneConfig::default());
        upsert_route(&state, route_proto(&["drone-a"]), "tester")
            .await
            .expect("upsert route");
        let mut p = profile();
        p.drone_tag = "drone-a".into();
        upsert_profile(&state, p, "tester").expect("upsert profile");

        assert!(delete_profile(&state, "drone-a", "tester").await.unwrap());
        // 关联清空的航线整体删除：否则会退化成适用于全部无人机的模板。
        assert!(state.routes.is_empty());
    }

    #[tokio::test]
    async fn deleting_profile_keeps_route_with_other_drones() {
        let state = DroneState::new(DroneConfig::default());
        upsert_route(&state, route_proto(&["drone-a", "drone-b"]), "tester")
            .await
            .expect("upsert route");
        let mut p = profile();
        p.drone_tag = "drone-a".into();
        upsert_profile(&state, p, "tester").expect("upsert profile");

        assert!(delete_profile(&state, "drone-a", "tester").await.unwrap());
        let route = state.routes.get("route-1").expect("route kept");
        assert_eq!(route.drone_tags, vec!["drone-b"]);
    }

    // -----------------------------------------------------------------------
    // 机载链路实体（adapter）
    // -----------------------------------------------------------------------

    fn adapter_proto() -> Adapter {
        Adapter {
            adapter_id: "adapter-1".into(),
            name: "东场链路".into(),
            protocol: "mavlink".into(),
            bind_addr: "0.0.0.0".into(),
            port_start: 14540,
            port_end: 14560,
            autopilot: "auto".into(),
            signing_secret: String::new(),
            signing_allow_unsigned: true,
            reconnect_min_secs: 1,
            reconnect_max_secs: 30,
            online: false,
            enabled: true,
            operator: String::new(),
        }
    }

    #[test]
    fn adapter_config_rejects_blank_id() {
        let mut a = adapter_proto();
        a.adapter_id = "   ".into();
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_rejects_unsafe_id_characters() {
        let mut a = adapter_proto();
        a.adapter_id = "adapter 1/../x".into();
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_rejects_inverted_port_range() {
        let mut a = adapter_proto();
        a.port_start = 20000;
        a.port_end = 10000;
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_rejects_port_out_of_range() {
        let mut a = adapter_proto();
        a.port_start = 70000;
        a.port_end = 70001;
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_rejects_inverted_reconnect_range() {
        let mut a = adapter_proto();
        a.reconnect_min_secs = 30;
        a.reconnect_max_secs = 1;
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_rejects_non_hex_signing_secret() {
        let mut a = adapter_proto();
        a.signing_secret = "zz".into();
        assert!(adapter_from_proto(&a).is_err());
    }

    #[test]
    fn adapter_config_fills_defaults_for_blank_fields() {
        let mut a = adapter_proto();
        a.protocol = String::new();
        a.autopilot = String::new();
        a.bind_addr = String::new();
        a.port_start = 0;
        a.port_end = 0;
        a.reconnect_min_secs = 0;
        a.reconnect_max_secs = 0;
        let row = adapter_from_proto(&a).expect("adapter");
        assert_eq!(row.protocol, "mavlink");
        assert_eq!(row.autopilot, "auto");
        assert_eq!(row.bind_addr, "0.0.0.0");
        assert_eq!(row.port_start, 14540);
        assert_eq!(row.port_end, 14560);
        assert_eq!(row.reconnect_min_secs, 1);
        assert_eq!(row.reconnect_max_secs, 30);
    }

    #[test]
    fn disabled_adapter_cannot_take_new_drones() {
        let mut row = AdapterRow {
            adapter_id: "adapter-1".into(),
            enabled: true,
            ..adapter_row_fixture()
        };
        assert!(ensure_adapter_assignable(&row).is_ok());
        row.enabled = false;
        assert!(ensure_adapter_assignable(&row).is_err());
    }

    #[test]
    fn upsert_keeps_disabled_state_and_runtime_status() {
        let existing = AdapterRow {
            enabled: false,
            online: true,
            ..adapter_row_fixture()
        };
        let mut incoming = adapter_proto();
        incoming.name = "改名".into();
        let row = merge_adapter(Some(&existing), &incoming).expect("merge");
        assert_eq!(row.name, "改名");
        assert!(!row.enabled, "启停状态只能由专门的启停接口改变");
        assert!(row.online, "在线状态由适配器注册维护，配置编辑不能覆盖");
    }

    #[test]
    fn upsert_creates_enabled_adapter_when_missing() {
        let row = merge_adapter(None, &adapter_proto()).expect("merge");
        assert!(row.enabled);
        assert!(!row.online);
    }

    #[test]
    fn adapter_proto_roundtrip_keeps_fields() {
        let row = adapter_from_proto(&adapter_proto()).expect("adapter");
        let back = adapter_to_proto(&row);
        assert_eq!(back.adapter_id, "adapter-1");
        assert_eq!(back.name, "东场链路");
        assert_eq!(back.port_start, 14540);
        assert_eq!(back.port_end, 14560);
        assert!(back.enabled);
    }
}
