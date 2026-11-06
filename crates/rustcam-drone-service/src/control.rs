//! 控制权仲裁（规则 §6.7）。
//!
//! 产品语义：飞行/云台是两个独立控制权；同一时刻同一设备同一类型只能有一个 holder。
//!
//! 仲裁必须「读-判断-写一次完成」，因此默认走 Redis Lua（`control.rs` 的 Lua 脚本见
//! `src/lua/`），跨实例互斥；Redis 不可用时降级为**进程内锁**并告警
//! （降级态只保证单实例互斥，见规则 §11.6 故障降级矩阵）。
//!
//! 关键规则：
//! - `acquire` 只在**无人持有或已过期**时成功，**不按优先级抢占**；
//! - `preempt` 才允许按优先级抢占，且要求优先级**严格更高**；
//! - 续约与释放只允许当前持有人；`force_release` 不受持有人限制。

use std::sync::Arc;

use rustcam_drone_core::event::DroneEvent;
use rustcam_drone_core::keys;
use rustcam_drone_core::metrics::names;

use crate::lease::LeaseScripts;
use crate::state::{AuditRecord, DroneState, LeaseRecord};

pub const CONTROL_FLIGHT: &str = "flight";
pub const CONTROL_GIMBAL: &str = "gimbal";

#[derive(Debug, Clone)]
pub struct LeaseOutcome {
    pub lease: LeaseRecord,
    pub granted: bool,
    pub reason: String,
    /// 抢占成功时为被抢占者（规则 §6.7）。
    pub preempted: String,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 控制权键：`control:{device_id}:{control_type}`（规则 §8.2）。
fn key(device_id: &str, control_type: &str) -> String {
    keys::control_key(device_id, control_type)
}

fn normalize_type(control_type: &str) -> String {
    match control_type.trim().to_ascii_lowercase().as_str() {
        "gimbal" | "camera" => CONTROL_GIMBAL.into(),
        _ => CONTROL_FLIGHT.into(),
    }
}

fn lease_secs(seconds: i32, default_secs: u32) -> i64 {
    let secs = if seconds <= 0 {
        default_secs as i64
    } else {
        seconds as i64
    };
    secs.clamp(5, 3_600)
}

fn broadcast(state: &DroneState, lease: &LeaseRecord, action: &str) {
    let _ = state.event_tx.send(DroneEvent::Control {
        device_id: lease.device_id.clone(),
        control_type: lease.control_type.clone(),
        holder: lease.holder.clone(),
        priority: lease.priority,
        expire_at_ms: lease.expire_at_ms,
        action: action.to_string(),
    });
}

/// 构造一条控制权审计记录。
fn audit_record(lease: &LeaseRecord, action: &str, result: &str, detail: &str) -> AuditRecord {
    AuditRecord {
        ts_ms: now_ms(),
        device_id: lease.device_id.clone(),
        category: "control".into(),
        action: action.to_string(),
        operator: lease.holder.clone(),
        result: result.to_string(),
        detail: detail.to_string(),
        control_type: lease.control_type.clone(),
        cmd_id: String::new(),
        params: String::new(),
        priority: lease.priority,
        finished_at_ms: 0,
    }
}

/// 本地缓存写入租约；Redis 为权威源时缓存只用于查询与过期广播。
fn cache_lease(state: &DroneState, lease: LeaseRecord) {
    state
        .leases
        .insert(key(&lease.device_id, &lease.control_type), lease);
}

fn cached_lease(state: &DroneState, device_id: &str, control_type: &str) -> Option<LeaseRecord> {
    state
        .leases
        .get(&key(device_id, control_type))
        .map(|l| l.clone())
}

/// 清理已过期的本地缓存并广播 `expired`（Redis 侧靠 TTL 自动消失）。
async fn clean_expired(state: &DroneState, device_id: &str, control_type: &str) {
    let k = key(device_id, control_type);
    let expired = state
        .leases
        .get(&k)
        .map(|l| l.expire_at_ms <= now_ms())
        .unwrap_or(false);
    if expired {
        if let Some((_, lease)) = state.leases.remove(&k) {
            state
                .write_audit(audit_record(&lease, "expire", "expired", "lease timeout"))
                .await;
            broadcast(state, &lease, "expired");
            state.persist();
        }
    }
}

/// 查询当前控制权。
pub async fn get(state: &DroneState, device_id: &str, control_type: &str) -> Option<LeaseRecord> {
    let control_type = normalize_type(control_type);
    clean_expired(state, device_id, &control_type).await;

    if let Some(redis) = state.redis.clone() {
        let k = key(device_id, &control_type);
        match state.scripts.read(&redis, &k).await {
            Ok(Some(lease)) => {
                let record = LeaseRecord {
                    device_id: device_id.to_string(),
                    control_type: control_type.clone(),
                    holder: lease.holder,
                    priority: lease.priority,
                    expire_at_ms: lease.expire_at_ms,
                };
                cache_lease(state, record.clone());
                return Some(record);
            }
            Ok(None) => {
                state.leases.remove(&k);
                return None;
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Control] redis read failed, using local cache");
            }
        }
    }

    cached_lease(state, device_id, &control_type)
}

/// 申请控制权：**只在无人持有或已过期时成功，不抢占**（规则 §6.7）。
pub async fn acquire(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    priority: i32,
    lease_seconds: i32,
) -> Result<LeaseOutcome, String> {
    if device_id.trim().is_empty() {
        return Err("device_id is required".into());
    }
    if operator.trim().is_empty() {
        return Err("operator is required".into());
    }
    let control_type = normalize_type(control_type);
    clean_expired(state, device_id, &control_type).await;

    let k = key(device_id, &control_type);
    let now = now_ms();
    let lease_ms = lease_secs(lease_seconds, state.config.default_lease_sec) * 1000;

    if let Some(redis) = state.redis.clone() {
        match state
            .scripts
            .acquire(&redis, &k, operator, priority, now, lease_ms)
            .await
        {
            Ok(result) => {
                if result.granted {
                    let lease = LeaseRecord {
                        device_id: device_id.to_string(),
                        control_type: control_type.clone(),
                        holder: operator.to_string(),
                        priority,
                        expire_at_ms: now + lease_ms,
                    };
                    cache_lease(state, lease.clone());
                    state
                        .write_audit(audit_record(&lease, "acquire", "granted", ""))
                        .await;
                    state.metrics_inc_labeled(
                        names::CONTROL_ACQUIRE_TOTAL,
                        &[("result", "granted")],
                    );
                    broadcast(state, &lease, "acquired");
                    state.persist();
                    return Ok(LeaseOutcome {
                        lease,
                        granted: true,
                        reason: "acquired".into(),
                        preempted: String::new(),
                    });
                }
                state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
                let lease = read_lease_cache(state, &k, result.other.clone(), priority, now);
                return Ok(LeaseOutcome {
                    lease,
                    granted: false,
                    reason: format!("控制权由 `{}` 持有", result.other),
                    preempted: String::new(),
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Control] redis acquire failed, degrading to local lock");
            }
        }
    }

    acquire_local(
        state,
        device_id,
        &control_type,
        operator,
        priority,
        lease_ms,
    )
    .await
}

/// 进程内降级仲裁：单实例互斥（Redis 不可用时）。
async fn acquire_local(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    priority: i32,
    lease_ms: i64,
) -> Result<LeaseOutcome, String> {
    // 临界区：读-判断-写一次完成，避免并发申请同时成功。
    let (outcome, audit) = {
        let _guard = state.lease_lock.lock().unwrap_or_else(|e| e.into_inner());
        let now = now_ms();

        if let Some(existing) = cached_lease(state, device_id, control_type) {
            if existing.expire_at_ms > now {
                state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
                return Ok(LeaseOutcome {
                    lease: existing.clone(),
                    granted: false,
                    reason: format!("控制权由 `{}` 持有", existing.holder),
                    preempted: String::new(),
                });
            }
        }

        let lease = LeaseRecord {
            device_id: device_id.to_string(),
            control_type: control_type.to_string(),
            holder: operator.to_string(),
            priority,
            expire_at_ms: now + lease_ms,
        };
        cache_lease(state, lease.clone());
        let audit = audit_record(&lease, "acquire", "granted", "");
        state.push_audit(audit.clone());
        state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "granted")]);
        broadcast(state, &lease, "acquired");
        state.persist();
        (
            LeaseOutcome {
                lease,
                granted: true,
                reason: "acquired".into(),
                preempted: String::new(),
            },
            audit,
        )
    };
    // 落库在临界区之外（MutexGuard 不能跨 await）。
    state.flush_audit_pg(&audit).await;
    Ok(outcome)
}

/// 抢占控制权：只有优先级**严格更高**才成功（规则 §6.7 / §17.2）。
pub async fn preempt(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    priority: i32,
    lease_seconds: i32,
) -> Result<LeaseOutcome, String> {
    if device_id.trim().is_empty() {
        return Err("device_id is required".into());
    }
    if operator.trim().is_empty() {
        return Err("operator is required".into());
    }
    let control_type = normalize_type(control_type);
    clean_expired(state, device_id, &control_type).await;

    let k = key(device_id, &control_type);
    let now = now_ms();
    let lease_ms = lease_secs(lease_seconds, state.config.default_lease_sec) * 1000;

    if let Some(redis) = state.redis.clone() {
        match state
            .scripts
            .preempt(&redis, &k, operator, priority, now, lease_ms)
            .await
        {
            Ok(result) => {
                if result.granted {
                    let lease = LeaseRecord {
                        device_id: device_id.to_string(),
                        control_type: control_type.clone(),
                        holder: operator.to_string(),
                        priority,
                        expire_at_ms: now + lease_ms,
                    };
                    cache_lease(state, lease.clone());
                    state
                        .write_audit(audit_record(
                            &lease,
                            "preempt",
                            "granted",
                            "higher priority",
                        ))
                        .await;
                    state.metrics_inc_labeled(
                        names::CONTROL_ACQUIRE_TOTAL,
                        &[("result", "preempted")],
                    );
                    state.metrics_inc(names::CONTROL_PREEMPT_TOTAL);
                    broadcast(state, &lease, "preempted");
                    state.persist();
                    return Ok(LeaseOutcome {
                        lease,
                        granted: true,
                        reason: "preempted".into(),
                        preempted: result.other,
                    });
                }
                state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
                let lease = read_lease_cache(state, &k, result.other.clone(), priority, now);
                return Ok(LeaseOutcome {
                    lease,
                    granted: false,
                    reason: format!("优先级不足：控制权由 `{}` 持有", result.other),
                    preempted: String::new(),
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Control] redis preempt failed, degrading to local lock");
            }
        }
    }

    preempt_local(
        state,
        device_id,
        &control_type,
        operator,
        priority,
        lease_ms,
    )
    .await
}

async fn preempt_local(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    priority: i32,
    lease_ms: i64,
) -> Result<LeaseOutcome, String> {
    let (outcome, audit) = {
        let _guard = state.lease_lock.lock().unwrap_or_else(|e| e.into_inner());
        let now = now_ms();

        if let Some(existing) = cached_lease(state, device_id, control_type) {
            if existing.expire_at_ms > now && priority <= existing.priority {
                state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "denied")]);
                return Ok(LeaseOutcome {
                    lease: existing.clone(),
                    granted: false,
                    reason: format!("优先级不足：控制权由 `{}` 持有", existing.holder),
                    preempted: String::new(),
                });
            }
            let mut lease = existing.clone();
            lease.holder = operator.to_string();
            lease.priority = priority;
            lease.expire_at_ms = now + lease_ms;
            cache_lease(state, lease.clone());
            let audit = audit_record(&lease, "preempt", "granted", "higher priority");
            state.push_audit(audit.clone());
            state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "preempted")]);
            state.metrics_inc(names::CONTROL_PREEMPT_TOTAL);
            broadcast(state, &lease, "preempted");
            state.persist();
            let preempted = existing.holder.clone();
            (
                LeaseOutcome {
                    lease,
                    granted: true,
                    reason: "preempted".into(),
                    preempted,
                },
                audit,
            )
        } else {
            let lease = LeaseRecord {
                device_id: device_id.to_string(),
                control_type: control_type.to_string(),
                holder: operator.to_string(),
                priority,
                expire_at_ms: now + lease_ms,
            };
            cache_lease(state, lease.clone());
            let audit = audit_record(&lease, "acquire", "granted", "");
            state.push_audit(audit.clone());
            state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "granted")]);
            broadcast(state, &lease, "acquired");
            state.persist();
            (
                LeaseOutcome {
                    lease,
                    granted: true,
                    reason: "acquired".into(),
                    preempted: String::new(),
                },
                audit,
            )
        }
    };
    // 落库在临界区之外（MutexGuard 不能跨 await）。
    state.flush_audit_pg(&audit).await;
    Ok(outcome)
}

/// 释放控制权；`force` 为真时不受持有人限制（管理员强制释放）。
pub async fn release(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    force: bool,
) -> Result<LeaseRecord, String> {
    let control_type = normalize_type(control_type);
    let k = key(device_id, &control_type);

    let existing = match get(state, device_id, &control_type).await {
        Some(lease) => lease,
        None => {
            return Ok(LeaseRecord {
                device_id: device_id.to_string(),
                control_type,
                holder: String::new(),
                priority: 0,
                expire_at_ms: 0,
            })
        }
    };

    if !force && existing.holder != operator {
        return Err(format!(
            "控制权由 `{}` 持有，不能由 `{operator}` 释放",
            existing.holder
        ));
    }

    if let Some(redis) = state.redis.clone() {
        match state.scripts.release(&redis, &k, operator, force).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "控制权由 `{}` 持有，不能由 `{operator}` 释放",
                    existing.holder
                ))
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Control] redis release failed, degrading to local lock");
            }
        }
    } else if !force {
        let _guard = state.lease_lock.lock().unwrap_or_else(|e| e.into_inner());
        let still_held = cached_lease(state, device_id, &control_type)
            .map(|l| l.holder == operator)
            .unwrap_or(false);
        if !still_held {
            return Err(format!(
                "控制权由 `{}` 持有，不能由 `{operator}` 释放",
                existing.holder
            ));
        }
    }

    state.leases.remove(&k);
    state
        .write_audit(audit_record(
            &existing,
            if force { "force_release" } else { "release" },
            "released",
            if force { "force" } else { "" },
        ))
        .await;
    broadcast(state, &existing, "released");
    state.persist();
    Ok(existing)
}

/// 续约：只允许当前持有人（规则 §6.7）。
pub async fn renew(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
    lease_seconds: i32,
) -> Result<LeaseOutcome, String> {
    let control_type = normalize_type(control_type);
    let k = key(device_id, &control_type);
    let now = now_ms();
    let lease_ms = lease_secs(lease_seconds, state.config.default_lease_sec) * 1000;

    if let Some(redis) = state.redis.clone() {
        match state
            .scripts
            .renew(&redis, &k, operator, now, lease_ms)
            .await
        {
            Ok(true) => {
                let lease = LeaseRecord {
                    device_id: device_id.to_string(),
                    control_type: control_type.clone(),
                    holder: operator.to_string(),
                    priority: cached_lease(state, device_id, &control_type)
                        .map(|l| l.priority)
                        .unwrap_or(0),
                    expire_at_ms: now + lease_ms,
                };
                cache_lease(state, lease.clone());
                state
                    .write_audit(audit_record(&lease, "renew", "granted", "same holder"))
                    .await;
                state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "renewed")]);
                state.persist();
                broadcast(state, &lease, "renewed");
                return Ok(LeaseOutcome {
                    lease,
                    granted: true,
                    reason: "renewed".into(),
                    preempted: String::new(),
                });
            }
            Ok(false) => {
                let holder = cached_lease(state, device_id, &control_type)
                    .map(|l| l.holder)
                    .unwrap_or_default();
                return Err(format!(
                    "控制权由 `{holder}` 持有，不能由 `{operator}` 续约"
                ));
            }
            Err(e) => {
                tracing::warn!(error = %e, "[Control] redis renew failed, degrading to local lock");
            }
        }
    }

    let (outcome, audit) = {
        let _guard = state.lease_lock.lock().unwrap_or_else(|e| e.into_inner());
        let Some(existing) = cached_lease(state, device_id, &control_type) else {
            return Err("控制权不存在".into());
        };
        if existing.holder != operator {
            return Err(format!(
                "控制权由 `{}` 持有，不能由 `{operator}` 续约",
                existing.holder
            ));
        }
        let mut lease = existing.clone();
        lease.expire_at_ms = now + lease_ms;
        cache_lease(state, lease.clone());
        let audit = audit_record(&lease, "renew", "granted", "same holder");
        state.push_audit(audit.clone());
        state.metrics_inc_labeled(names::CONTROL_ACQUIRE_TOTAL, &[("result", "renewed")]);
        state.persist();
        (
            LeaseOutcome {
                lease,
                granted: true,
                reason: "renewed".into(),
                preempted: String::new(),
            },
            audit,
        )
    };
    // 落库在临界区之外（MutexGuard 不能跨 await）。
    broadcast(state, &outcome.lease, "renewed");
    state.flush_audit_pg(&audit).await;
    Ok(outcome)
}

/// 飞行/云台指令下发前的控制权校验。
pub async fn ensure_holder(
    state: &DroneState,
    device_id: &str,
    control_type: &str,
    operator: &str,
) -> Result<(), String> {
    if !state.config.require_control {
        return Ok(());
    }
    let control_type = normalize_type(control_type);
    let Some(lease) = get(state, device_id, &control_type).await else {
        return Err("未持有飞行控制权，请先申请控制权".into());
    };
    if lease.holder != operator {
        return Err(format!("控制权由 `{}` 持有", lease.holder));
    }
    Ok(())
}

/// 后台清理过期租约（本地缓存）。
pub async fn reap_expired(state: &DroneState) -> usize {
    let now = now_ms();
    let expired: Vec<String> = state
        .leases
        .iter()
        .filter(|e| e.value().expire_at_ms <= now)
        .map(|e| e.key().clone())
        .collect();

    let mut count = 0;
    for k in expired {
        if let Some((_, lease)) = state.leases.remove(&k) {
            state
                .write_audit(audit_record(&lease, "expire", "expired", "lease timeout"))
                .await;
            broadcast(state, &lease, "expired");
            count += 1;
        }
    }
    if count > 0 {
        state.persist();
    }
    count
}

/// 构造一条只用于展示的租约（Redis 拒绝时用对方持有人填充）。
fn read_lease_cache(
    state: &DroneState,
    k: &str,
    holder: String,
    priority: i32,
    now: i64,
) -> LeaseRecord {
    if let Some(cached) = state.leases.get(k) {
        return cached.clone();
    }
    LeaseRecord {
        device_id: k.split(':').nth(1).unwrap_or_default().to_string(),
        control_type: k.split(':').nth(2).unwrap_or_default().to_string(),
        holder,
        priority,
        expire_at_ms: now,
    }
}

/// 预热 Redis 脚本（启动时调用；失败不阻断启动）。
pub async fn preload_scripts(state: &DroneState) {
    if let Some(redis) = state.redis.clone() {
        let scripts: Arc<LeaseScripts> = state.scripts.clone();
        if let Err(e) = scripts.preload(&redis).await {
            tracing::warn!(error = %e, "[Control] lua scripts preload failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DroneConfig;

    fn state() -> DroneState {
        DroneState::new(DroneConfig {
            require_control: true,
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn first_acquire_wins_and_second_is_denied() {
        let s = state();
        let a = acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();
        assert!(a.granted);
        let b = acquire(&s, "d1", "flight", "bob", 10, 30).await.unwrap();
        assert!(!b.granted);
        assert_eq!(b.lease.holder, "alice");
        assert_eq!(s.recent_audits("d1", 10).len(), 1);
    }

    #[tokio::test]
    async fn only_holder_can_release_unless_forced() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();
        assert!(release(&s, "d1", "flight", "bob", false).await.is_err());
        assert!(release(&s, "d1", "flight", "alice", false).await.is_ok());
        assert!(get(&s, "d1", "flight").await.is_none());
    }

    #[tokio::test]
    async fn ensure_holder_blocks_other_operator() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();
        assert!(ensure_holder(&s, "d1", "flight", "alice").await.is_ok());
        assert!(ensure_holder(&s, "d1", "flight", "bob").await.is_err());
    }

    #[tokio::test]
    async fn expired_lease_is_reaped() {
        let s = state();
        let mut lease = LeaseRecord {
            device_id: "d1".into(),
            control_type: "flight".into(),
            holder: "alice".into(),
            priority: 1,
            expire_at_ms: now_ms() - 1,
        };
        s.leases.insert(key("d1", "flight"), lease.clone());
        assert_eq!(reap_expired(&s).await, 1);
        assert!(get(&s, "d1", "flight").await.is_none());

        lease.expire_at_ms = now_ms() + 10_000;
        s.leases.insert(key("d1", "flight"), lease);
        assert_eq!(reap_expired(&s).await, 0);
    }

    #[tokio::test]
    async fn require_control_false_allows_commands() {
        let s = DroneState::new(DroneConfig {
            require_control: false,
            ..Default::default()
        });
        assert!(ensure_holder(&s, "d1", "flight", "anyone").await.is_ok());
    }

    // ---------- 规则 §6.7 / §17.2：acquire 不抢占，preempt 才按优先级抢占 ----------

    #[tokio::test]
    async fn acquire_never_preempts_higher_priority() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();

        let b = acquire(&s, "d1", "flight", "bob", 20, 30).await.unwrap();

        assert!(!b.granted, "acquire 不得按优先级抢占");
        assert_eq!(
            get(&s, "d1", "flight").await.unwrap().holder,
            "alice",
            "控制权仍归原持有人"
        );
    }

    #[tokio::test]
    async fn preempt_requires_strictly_higher_priority() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();

        let same = preempt(&s, "d1", "flight", "bob", 10, 30).await.unwrap();
        assert!(!same.granted, "同优先级不得抢占");

        let lower = preempt(&s, "d1", "flight", "bob", 5, 30).await.unwrap();
        assert!(!lower.granted, "低优先级不得抢占");

        let higher = preempt(&s, "d1", "flight", "bob", 20, 30).await.unwrap();
        assert!(higher.granted, "更高优先级应抢占成功");
        assert_eq!(higher.preempted, "alice", "必须返回被抢占者");
        assert_eq!(get(&s, "d1", "flight").await.unwrap().holder, "bob");
    }

    #[tokio::test]
    async fn renew_only_by_holder() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();

        assert!(
            renew(&s, "d1", "flight", "bob", 30).await.is_err(),
            "非持有人不得续约"
        );
        assert!(renew(&s, "d1", "flight", "alice", 30).await.is_ok());
    }

    #[tokio::test]
    async fn concurrent_acquire_has_exactly_one_winner() {
        let s = std::sync::Arc::new(state());
        let mut handles = Vec::new();
        for i in 0..8 {
            let s = s.clone();
            handles.push(tokio::spawn(async move {
                acquire(&s, "d1", "flight", &format!("op-{i}"), 10, 30).await
            }));
        }

        let mut granted = 0;
        for h in handles {
            let outcome = h.await.unwrap().unwrap();
            if outcome.granted {
                granted += 1;
            }
        }

        assert_eq!(granted, 1, "并发申请控制权只能有一个赢家");
    }

    #[tokio::test]
    async fn lease_key_follows_rule_namespace() {
        let s = state();
        acquire(&s, "d1", "flight", "alice", 10, 30).await.unwrap();
        assert!(
            s.leases.contains_key("control:d1:flight"),
            "控制权键必须为 control:device_id:control_type"
        );
    }
}
