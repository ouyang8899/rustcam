# 移除 device_id、统一 drone_tag 主键 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除无人机域的 `device_id` 概念，平台主键统一为 `drone_tag`，链路绑定改由 `(adapter_id, mavlink_sysid)` 表达，adapter 改为「先注册 → 收 assignment → 一机一连建链」的多机连接池。

**Architecture:** 协议字段 `device_id` → `drone_tag`（字段号不变、名字换），一次性切换不留双键。服务端内存/Redis/DB 的设备键全部改为 `drone_tag`；`drone_device` 表新增 `(adapter_id, mavlink_sysid)` 唯一索引作为链路绑定。adapter 侧删除本地 `device_id`/`devices`/`mavlink_addr` 配置，注册后由服务端按 `adapter_id` 分配端点与密钥，逐条建链并按连接归属遥测。

**Tech Stack:** Rust 1.95+（edition 2021）、tokio、axum、tonic/prost（gRPC，proto 生成代码）、rbatis（PostgreSQL）、redis、Vue 3 + Element Plus（前端）、Python + psycopg2（SQL 迁移脚本）。

**Spec:** `docs/superpowers/specs/2026-11-07-remove-device-id-design.md`（本计划与该规格一一对应；执行者需同时阅读）

## Global Constraints

- **命名**：无人机域标识一律用 `drone_tag`（值形如 `drone-<uuid4>`，保留连字符）；链路期望 sysid/compid 字段名保持 `mavlink_sysid` / `mavlink_compid`。
- **域边界**：`rustcam-media` 里 `device_id` 有两套语义——**无人机域**（`src/drone_gateway/**`、`tests/drone_auth_guard.rs`）要改；**摄像机域**（`src/application/device_service.rs`、`src/protocol/event.rs`、`src/infrastructure/db_repository.rs`、`src/sql_mappers.rs`、`src/media/tcp_rtsp_rtp_server.rs`、`src/application/{ws_broadcaster,play_service,recording_scheduler_service,hook_event_handler,stream_recovery_service,stream_service}.rs`、`src/domain/{traits,ptz,alarm}.rs`、`src/transport/{sip_tcp_server,sip_udp_server}.rs`、`src/protocol/adapter_event_handler.rs`）**一个字都不许改**。
- **proto 字段号不变**：只改字段名 `device_id` → `drone_tag`；`repeated device_ids` → `repeated drone_tags`；`RegisterAck.assigned_devices` → `assigned_drone_tags`；删除 `Register.devices`、`Register.identities`；`DroneProfile.device_id`(1) 与 `DroneProfile.drone_tag`(21) 同义合并为 `drone_tag`(1)。
- **TDD**：每个任务先写失败测试并跑到失败，再写实现，再跑到通过。禁止"先写实现再补测试"。
- **每个任务结束时工作区必须可编译**：`cargo check --workspace` 无错误。跨任务的中间态不允许留破仓库。
- **质量门**：`cargo fmt --all`、`cargo test -p <crate>` 全绿；clippy 只对本次涉及的 crate 跑（`rustcam-media` 存在 308 个与本任务无关的既存 clippy 报错，**不要去修**）。
- **已知既存失败（勿追）**：`rustcam-media` 的 `protocol::gb28181::adapter::tests::test_parse_xml_field_with_special_chars_in_content` 失败，与本任务无关。
- **环境坑**：若 prost-build 报 `%TEMP%` 权限错误，先 `$env:TEMP='D:\RustCam-Media\.tmp-cargo'; $env:TMP=$env:TEMP` 再执行 cargo 命令。
- **提交**：每个任务结束时提交一次，提交信息用 Conventional Commits（如 `refactor(drone): 统一以 drone_tag 作为设备主键`）；**只 add 本任务涉及的文件**。
- **语言**：代码注释与用户可见文案用中文。

---

### Task 0: 提交已落地的前置修复

上一轮已完成的改动还在工作区未提交（其中 `drone-service/src/admin.rs` 与 `rustcam-media/src/drone_gateway/client.rs` 的改动会被后续任务覆盖，先提交留痕）。

**Files:**
- Modify: `README.md`
- Modify: `crates/rustcam-drone-adapter/src/link/mod.rs`
- Modify: `crates/rustcam-drone-service/src/admin.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/admin.rs`
- Modify: `crates/rustcam-media/src/drone_gateway/client.rs`
- Modify: `crates/rustcam-media/src/drone_gateway/ws.rs`
- Modify: `drone.toml`
- Create: `docs/superpowers/specs/2026-11-07-remove-device-id-design.md`
- Create: `docs/superpowers/plans/2026-11-07-remove-device-id.md`

**Interfaces:**
- Consumes: 无
- Produces: 干净的工作区（后续任务从零开始叠加）。

- [ ] **Step 1: 确认工作区改动与预期一致**

Run: `git status --short`
Expected: 恰为下列条目（其余未跟踪文件保持不动）：
```
 M README.md
 M crates/rustcam-drone-adapter/src/link/mod.rs
 M crates/rustcam-drone-service/src/admin.rs
 M crates/rustcam-drone-service/src/grpc/admin.rs
 M crates/rustcam-media/src/drone_gateway/client.rs
 M crates/rustcam-media/src/drone_gateway/ws.rs
 M drone.toml
```

- [ ] **Step 2: 跑相关测试确认绿**

Run: `cargo test -p rustcam-drone-adapter -p rustcam-drone-service` 与 `cargo test -p rustcam-media --lib drone_gateway`
Expected: 全部 PASS。

- [ ] **Step 3: 提交**

```bash
git add README.md crates/rustcam-drone-adapter/src/link/mod.rs \
  crates/rustcam-drone-service/src/admin.rs crates/rustcam-drone-service/src/grpc/admin.rs \
  crates/rustcam-media/src/drone_gateway/client.rs crates/rustcam-media/src/drone_gateway/ws.rs \
  drone.toml docs/superpowers
git commit -m "feat(drone): 适配器上报唯一标识，档案支持未指定寻址参数回退"
```

---

### Task 1: `rustcam-drone-core` 共享类型改名

**Files:**
- Modify: `crates/rustcam-drone-core/src/keys.rs`
- Modify: `crates/rustcam-drone-core/src/telemetry.rs`
- Modify: `crates/rustcam-drone-core/src/event.rs`
- Modify: `crates/rustcam-drone-core/src/command.rs`

**Interfaces:**
- Consumes: 无
- Produces:
  - `keys::control_key(drone_tag: &str, control_type: &str) -> String` → `"control:{drone_tag}:{control_type}"`
  - `keys::shadow_key(drone_tag: &str) -> String` → `"shadow:{drone_tag}"`
  - `keys::adapter_device_key(drone_tag: &str) -> String` → `"adapter:device:{drone_tag}"`
  - `telemetry::Telemetry` 字段 `device_id` → `drone_tag: String`（`is_valid()` 内的判空同步改名）
  - `event::DroneEvent` 字段 `device_id` → `drone_tag: String`
  - `command::Command` 字段 `device_id` → `drone_tag: String`

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-core/src/keys.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn keys_are_built_from_drone_tag() {
        assert_eq!(control_key("drone-a", "flight"), "control:drone-a:flight");
        assert_eq!(shadow_key("drone-a"), "shadow:drone-a");
        assert_eq!(adapter_device_key("drone-a"), "adapter:device:drone-a");
    }
```

在 `crates/rustcam-drone-core/src/telemetry.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn telemetry_carries_drone_tag() {
        let t = Telemetry {
            drone_tag: "drone-a".into(),
            ..Default::default()
        };
        assert_eq!(t.drone_tag, "drone-a");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-core`
Expected: FAIL —— `no field 'drone_tag' on type 'Telemetry'`。

- [ ] **Step 3: 改实现**

四个文件里把结构体字段与函数参数名 `device_id` 改为 `drone_tag`（键前缀字符串不变）：

```rust
pub fn control_key(drone_tag: &str, control_type: &str) -> String {
    format!("{CONTROL_KEY_PREFIX}{drone_tag}:{control_type}")
}
```

`keys.rs` 里已有的 `keys_are_namespaced` 用例的入参改为 `drone_tag` 命名（如 `control_key("drone-001", "flight")`），断言值不变。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-core`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-core
git commit -m "refactor(drone-core): 共享类型的 device_id 改为 drone_tag"
```

---

### Task 2: proto 字段改名

**Files:**
- Modify: `crates/rustcam-drone-proto/proto/common.proto`
- Modify: `crates/rustcam-drone-proto/proto/adapter_link.proto`
- Modify: `crates/rustcam-drone-proto/proto/control.proto`
- Modify: `crates/rustcam-drone-proto/src/lib.rs`

**Interfaces:**
- Consumes: 无
- Produces: 生成类型里所有 `device_id` 字段变为 `drone_tag`；`Register` 去掉 `devices` 与 `identities`；`RegisterAck.assigned_devices` 变为 `assigned_drone_tags`；`DroneProfile` 只保留一个 `drone_tag`（字段号 1，删除原 21）。

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-proto/src/lib.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn telemetry_carries_drone_tag() {
        let t = common::Telemetry {
            drone_tag: "drone-a".into(),
            ..Default::default()
        };
        assert_eq!(t.drone_tag, "drone-a");
    }

    #[test]
    fn register_ack_lists_drone_tags() {
        let ack = link::RegisterAck {
            accepted: true,
            reason: String::new(),
            assigned_drone_tags: vec!["drone-a".into()],
            heartbeat_interval_sec: 10,
            assignments: vec![link::DeviceAssignment {
                drone_tag: "drone-a".into(),
                mavlink_addr: "udpin:10.88.0.2:14540".into(),
                signing_secret: String::new(),
                allow_unsigned: true,
                mavlink_version: 2,
                mavlink_sysid: 1,
                mavlink_compid: 1,
            }],
        };
        assert_eq!(ack.assigned_drone_tags[0], "drone-a");
        assert_eq!(ack.assignments[0].drone_tag, "drone-a");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-proto`
Expected: FAIL —— `no field 'drone_tag'`。

- [ ] **Step 3: 改 proto**

`common.proto`：`Telemetry.device_id`→`drone_tag`(1)、`Command.device_id`→`drone_tag`(2)、`CommandReply.device_id`→`drone_tag`(2)、`DeviceEvent.device_id`→`drone_tag`(2)。

`adapter_link.proto`：
- `Register`：删掉 `repeated string devices = 4;` 与 `repeated DeviceIdentity identities = 5;`。
- `DeviceIdentity.device_id` → `drone_tag`。
- `DeviceAssignment.device_id` → `drone_tag`。
- `RegisterAck.assigned_devices` → `assigned_drone_tags`。
- `ControlState.device_id` → `drone_tag`。

`control.proto`：所有 `string device_id = N;` → `string drone_tag = N;`，`repeated string device_ids = N;` → `repeated string drone_tags = N;`；`DroneProfile` 删除原 21 号 `drone_tag`（与 1 号同义），保留 `drone_tag = 1`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-proto`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-proto
git commit -m "refactor(drone-proto): 协议字段 device_id 统一改为 drone_tag"
```

---

### Task 3: drone-service 内存状态键改名

**Files:**
- Modify: `crates/rustcam-drone-service/src/state.rs`
- Modify: `crates/rustcam-drone-service/src/shadow.rs`
- Modify: `crates/rustcam-drone-service/src/presence.rs`
- Modify: `crates/rustcam-drone-service/src/event.rs`
- Modify: `crates/rustcam-drone-service/src/store.rs`
- Modify: `crates/rustcam-drone-service/src/risk.rs`

**Interfaces:**
- Consumes: Task 1 的 `Telemetry.drone_tag` / `DroneEvent.drone_tag`、Task 2 的 proto `drone_tag`。
- Produces:
  - `state.devices: DashMap<String, DeviceEntry>`，键为 `drone_tag`
  - `state.profiles: DashMap<String, DroneProfileRecord>`，键为 `drone_tag`
  - `state.device_to_adapter: DashMap<String, String>`（drone_tag → adapter_id）
  - `DeviceEntry.device_id` → `drone_tag`；`DeviceProfileRecord.device_id` → `drone_tag`
  - `shadow::ingest(state, telemetry)` 按 `telemetry.drone_tag` 落设备条目
  - `store::Snapshot` 里 profile 字段 `device_id` → `drone_tag`

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-service/src/shadow.rs` 的 `mod tests` 内追加（沿用该文件已有的 `telemetry()` 辅助函数）：

```rust
    #[test]
    fn telemetry_is_stored_under_drone_tag() {
        let state = DroneState::new(DroneConfig::default());
        let mut t = telemetry(1, 1_000);
        t.drone_tag = "drone-a".into();
        let out = ingest(&state, t);
        assert!(out.accepted());
        let entry = state.devices.get("drone-a").expect("应当按 drone_tag 落库");
        assert_eq!(entry.drone_tag, "drone-a");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-service shadow`
Expected: FAIL —— 字段名不匹配（`no field 'drone_tag'`）。

- [ ] **Step 3: 改实现**

这 6 个文件里把 `device_id` 语义统一改为 `drone_tag`（结构体字段名、map 键、函数参数、快照 JSON 字段名）。

`store.rs` 的 `Snapshot` 是 serde 持久化结构：给 `drone_tag` 加 `#[serde(default)]`；`store::load` 遇到缺失 `drone_tag` 的老记录时留空（由服务端建档时重新分配），不要报错。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-service`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-service
git commit -m "refactor(drone-service): 设备状态键统一改为 drone_tag"
```

---

### Task 4: drone-service 注册与分配

**Files:**
- Modify: `crates/rustcam-drone-service/src/registry.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/adapter_link.rs`
- Modify: `crates/rustcam-drone-service/src/infra/db.rs`（`DeviceLinkRow` 与 `load_device_links`）

**Interfaces:**
- Consumes: Task 2 的 `RegisterAck.assigned_drone_tags` / `DeviceAssignment.drone_tag`。
- Produces:
  - `infra::db::DeviceLinkRow { drone_tag, mavlink_addr, signing_secret, signing_allow_unsigned, mavlink_version, mavlink_sysid, mavlink_compid }`（`device_id` 字段删除）
  - `infra::db::load_device_links(adapter_id) -> Vec<DeviceLinkRow>`（查 `drone_device WHERE adapter_id = ?`，返回 `drone_tag`）
  - `Registry::register(&self, reg: &Register, tx: AdapterTx) -> RegisterAck`：分配集合 = 该 adapter 名下 `drone_tag` 列表；Redis 独占键 `adapter:device:{drone_tag}`；**注册阶段不做唯一标识校验**；**无 DB 记录时不再回落到 adapter 自报**（引导模式取消）

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-service/src/registry.rs` 的 `mod tests` 内追加，并把该文件既有测试里的 `identities: Vec::new()`、`assigned_devices` 断言改为新字段名：

```rust
    #[tokio::test]
    async fn register_without_db_assigns_nothing() {
        let r = registry();
        let (tx, _rx) = channel();
        let mut reg = register_msg("a-1", vec!["drone-001"]);
        reg.devices.clear();
        let ack = r.register(&reg, tx).await;
        assert!(ack.accepted);
        assert!(ack.assigned_drone_tags.is_empty(), "引导模式已取消：无库时不应分配");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-service registry`
Expected: FAIL —— `no field 'devices'` / `no field 'assigned_drone_tags'`。

- [ ] **Step 3: 改实现**

- `registry.rs`：`Register` 不再带设备名单 → 分配来源只有 `load_link_rows(&reg.adapter_id)`；无行时分配空列表并 `tracing::warn!("[Registry] adapter `{id}` 在平台没有绑定无人机")`。
- 删除注册阶段的唯一标识校验（原读 `reg.identities` 的那段），校验只保留 `apply_identity_report` 与指令前两处。
- `build_assignment` 用 `row.drone_tag` 填 `DeviceAssignment.drone_tag`。
- `infra/db.rs`：`DeviceLinkRow` 字段改名；`load_device_links` 的 SQL 改查 `drone_tag`；`upsert_device_link` 的 SQL 列名同步。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-service`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-service
git commit -m "refactor(drone-service): 注册按 adapter_id 分配 drone_tag 与链路参数"
```

---

### Task 5: drone-service 建档与唯一性约束

**Files:**
- Modify: `crates/rustcam-drone-service/src/admin.rs`
- Modify: `crates/rustcam-drone-service/src/state.rs`（`DroneProfileRecord` 字段）

**Interfaces:**
- Consumes: Task 3/4 的 `drone_tag` 键。
- Produces:
  - `admin::upsert_profile(state, profile, operator) -> Result<DroneProfileRecord, String>`：入参 `profile.drone_tag` 为空则生成 `drone-<uuid4>`；已存在则更新且**不可变**（传入不同值报错）
  - 新增校验：`(adapter_id, mavlink_sysid)` 与其他档案冲突 → `Err("adapter_id/mavlink_sysid 已被占用")`
  - `mavlink_sysid`/`mavlink_compid` 为 0 时保留已有值 / 默认 1（沿用已实现的 `pick_or_keep`）

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-service/src/admin.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn drone_tag_is_generated_when_absent() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.drone_tag = String::new();
        let rec = upsert_profile(&state, p, "tester").expect("应当保存成功");
        assert!(rec.drone_tag.starts_with("drone-"));
    }

    #[test]
    fn duplicate_adapter_sysid_is_rejected() {
        let state = DroneState::new(DroneConfig::default());
        let mut p = profile();
        p.drone_tag = "drone-a".into();
        p.adapter_id = "adapter-1".into();
        p.mavlink_sysid = 12;
        upsert_profile(&state, p, "tester").expect("首次应当成功");

        let mut p2 = profile();
        p2.drone_tag = "drone-b".into();
        p2.adapter_id = "adapter-1".into();
        p2.mavlink_sysid = 12;
        assert!(upsert_profile(&state, p2, "tester").is_err(), "同 adapter 内 sysid 不得重复");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-service admin`
Expected: FAIL —— `no field 'drone_tag'`。

- [ ] **Step 3: 改实现**

- `admin.rs`：`upsert_profile` 的 `device_id` 语义改 `drone_tag`；新增 `(adapter_id, mavlink_sysid)` 冲突检查（在 `state.profiles` 内线性比对即可）；`resolve_drone_tag` 逻辑并入（传空生成、传已存在值保持、传不同值报错）。
- `state.rs`：`DroneProfileRecord.device_id` → `drone_tag`；`DroneProfileRecord::new` 生成 `drone-<uuid4>`。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-service`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-service
git commit -m "feat(drone-service): 档案以 drone_tag 为主键并约束 adapter+sysid 唯一"
```

---

### Task 6: drone-service 指令、控制权、围栏、审计

**Files:**
- Modify: `crates/rustcam-drone-service/src/commands.rs`
- Modify: `crates/rustcam-drone-service/src/control.rs`
- Modify: `crates/rustcam-drone-service/src/geofence.rs`
- Modify: `crates/rustcam-drone-service/src/query.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/control.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/geofence.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/query.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/event.rs`
- Modify: `crates/rustcam-drone-service/src/grpc/admin.rs`

**Interfaces:**
- Consumes: Task 3/4/5 的 `drone_tag` 键与字段。
- Produces:
  - `commands::check_identity(state, drone_tag) -> Result<(), String>`（按 `drone_tag` 取档案与实测值）
  - `proto::Command.drone_tag` 贯穿下发链路；租约键 `control:{drone_tag}:{type}`
  - `Route`/`Fence` 的 `device_ids` → `drone_tags`

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-service/src/commands.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn identity_check_uses_drone_tag() {
        let state = DroneState::new(DroneConfig::default());
        state.profiles.insert(
            "drone-a".into(),
            crate::state::DroneProfileRecord {
                drone_tag: "drone-a".into(),
                unique_id: "registered".into(),
                ..crate::state::DroneProfileRecord::new("drone-a", 0)
            },
        );
        state.devices.insert(
            "drone-a".into(),
            crate::state::DeviceEntry::new("drone-a", "mavlink", "a-1", 0),
        );
        state.devices.get_mut("drone-a").unwrap().unique_id = "observed".into();
        assert!(check_identity(&state, "drone-a").is_err(), "实测不一致应当拒绝");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-service commands`
Expected: FAIL —— 字段名不匹配。

- [ ] **Step 3: 改实现**

这些文件里的 `device_id` 语义改为 `drone_tag`（字段名、参数名、map 键、审计记录字段、proto 字段引用）。围栏/航线的 `device_ids` → `drone_tags`（含 `FenceRecord`/`RouteRecord` 的 serde 字段名，加 `#[serde(default)]` 兼容旧快照）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-service`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-service
git commit -m "refactor(drone-service): 指令与控制权链路改用 drone_tag"
```

---

### Task 7: drone-service 数据库层与表结构

**Files:**
- Modify: `crates/rustcam-drone-service/migrations.sql`
- Modify: `crates/rustcam-drone-service/src/infra/db.rs`（`write_audit`、`upsert_route`、`upsert_fence` 等 SQL 列名）

**Interfaces:**
- Consumes: Task 4/5 的 `drone_tag` 列名。
- Produces: `drone_device` 新结构（主键 `drone_tag`，无 `device_id` 列，唯一索引 `(adapter_id, mavlink_sysid)`）；`drone_control_audit` / `drone_command_audit` / `drone_telemetry` 的列 `drone_tag`。

> `sql/schema.sql` 无需改动：它只含 `drone_camera`（已按 `drone_tag` 组织，且无指向 `drone_device` 的外键）。

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-service/src/infra/db.rs` 的测试模块内追加：

```rust
    #[test]
    fn schema_uses_drone_tag_and_no_device_id() {
        let sql = include_str!("../migrations.sql");
        assert!(sql.contains("drone_tag"));
        assert!(!sql.contains("device_id"));
        assert!(sql.contains("uq_drone_device_adapter_sysid"));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-service infra::db`
Expected: FAIL —— migrations.sql 仍含 `device_id`。

- [ ] **Step 3: 改实现**

- `migrations.sql`：`drone_device` 改为

```sql
CREATE TABLE IF NOT EXISTS drone_device (
  drone_tag    text PRIMARY KEY,
  protocol     text NOT NULL,
  adapter_id   text,
  online       boolean NOT NULL DEFAULT false,
  last_seen    timestamptz,
  device_tag   text,
  channel_tag  text,
  video_stream text,
  unique_id     text,
  mavlink_addr  text,
  signing_secret text,
  signing_allow_unsigned boolean NOT NULL DEFAULT true,
  mavlink_version smallint NOT NULL DEFAULT 2,
  mavlink_sysid smallint NOT NULL DEFAULT 1,
  mavlink_compid smallint NOT NULL DEFAULT 1,
  meta          jsonb,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_drone_device_adapter_sysid
    ON drone_device (adapter_id, mavlink_sysid)
    WHERE adapter_id <> '' AND mavlink_sysid > 0;
```

  三张审计/遥测表列名 `device_id` → `drone_tag`。
- `infra/db.rs`：所有 SQL 文本里的 `device_id` 列名与绑定参数同步改名。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-service`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-service
git commit -m "refactor(drone-service): 无人机表结构改用 drone_tag 主键"
```

---

### Task 8: rustcam-media DTO 改名

**Files:**
- Modify: `crates/rustcam-media/src/drone_gateway/client.rs`

**Interfaces:**
- Consumes: Task 2 的 proto `drone_tag` 字段。
- Produces:
  - `DroneDto`：删除 `device_id`，保留 `drone_tag`（主键）
  - `TelemetryDto.device_id` → `drone_tag`（已含 `unique_id`）
  - `CommandDto`/`CommandRequest`/`ControlDto`/`ControlRequest`/`RouteDto`/`FenceDto`/`AlertDto`/`AuditDto`/`DroneEventDto` 的 `device_id` → `drone_tag`；`device_ids` → `drone_tags`

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-media/src/drone_gateway/client.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn telemetry_dto_carries_drone_tag() {
        let dto = telemetry_dto(proto::Telemetry {
            drone_tag: "drone-a".into(),
            unique_id: "0011223344556677".into(),
            ..Default::default()
        });
        assert_eq!(dto.drone_tag, "drone-a");
        assert_eq!(dto.unique_id, "0011223344556677");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-media --lib drone_gateway::client`
Expected: FAIL —— `no field 'drone_tag' on type 'TelemetryDto'`。

- [ ] **Step 3: 改实现**

改 `client.rs` 里 `drone_gateway` 部分的 DTO 定义与 `drone_dto`/`profile_dto`/`profile_msg`/`telemetry_dto` 等转换函数；`DroneDto` 删除 `device_id` 字段（`drone_tag` 已是主键）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-media --lib drone_gateway`
Expected: PASS（既存的 GB28181 用例失败与本任务无关，忽略）。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-media/src/drone_gateway
git commit -m "refactor(drone-gateway): DTO 统一使用 drone_tag"
```

---

### Task 9: rustcam-media REST 路由与权限映射

**Files:**
- Modify: `crates/rustcam-media/src/drone_gateway/api.rs`
- Modify: `crates/rustcam-media/src/drone_gateway/permissions.rs`
- Modify: `crates/rustcam-media/tests/drone_auth_guard.rs`

**Interfaces:**
- Consumes: Task 8 的 DTO。
- Produces: 所有无人机路由的路径参数为 `{drone_tag}`；请求体字段 `drone_tag`；查询参数 `drone_tag`；`permissions.rs` 的路径→权限映射表同步。

- [ ] **Step 1: 写失败测试**

把 `crates/rustcam-media/tests/drone_auth_guard.rs` 里既有 drone 路由用例的路径参数改成 `drone-a`（drone_tag 形态），并追加一条用例：

```rust
// drone 路由以 drone_tag 定位：路径参数换成 drone_tag 后仍按 drone 权限放行/拦截
// 参考同文件既有用例的调用方式（如 request().get("/api/v1/drone/drones/<id>")）
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-media --test drone_auth_guard`
Expected: FAIL —— 路由参数与权限表不匹配。

- [ ] **Step 3: 改实现**

- `api.rs`：路由 `/api/v1/drone/drones/{device_id}` → `{drone_tag}`、`/api/v1/drone/devices/{device_id}` → `{drone_tag}`、`/api/v1/drone/{device_id}/command` → `{drone_tag}`、`/api/v1/drone/{device_id}/telemetry` → `{drone_tag}`；处理器入参 `device_id` → `drone_tag`；`ControlRequest.device_id` → `drone_tag`。
- `permissions.rs`：路径表里的 `{device_id}` 占位改成 `{drone_tag}`（权限集合不变）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-media --test drone_auth_guard` 与 `cargo test -p rustcam-media --lib drone_gateway`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-media/src/drone_gateway crates/rustcam-media/tests
git commit -m "refactor(drone-gateway): REST 路径参数改为 drone_tag"
```

---

### Task 10: adapter 配置去掉本地 device_id / devices

**Files:**
- Modify: `crates/rustcam-drone-adapter/src/config.rs`
- Modify: `crates/rustcam-drone-adapter/src/main.rs`
- Modify: `adapter.toml.example`

**Interfaces:**
- Consumes: 无
- Produces: `AdapterConfig` 删除 `device_id` 与 `devices` 两个字段、删除 `effective_device_id()` 与 `registration_devices()`；`mavlink_addr` **本任务改为可选**（`validate()` 不再要求非空，字段保留到 Task 11 再删）。环境变量 `DEVICE_ID` / `DEVICES` 一并移除。

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-adapter/src/config.rs` 的 `mod tests` 内追加：

```rust
    #[test]
    fn endpoint_is_not_mandatory_locally() {
        let mut cfg = AdapterConfig::default();
        cfg.mavlink_addr = String::new();
        // 端点改由服务端分配，本地不再必填
        cfg.validate().expect("端点由服务端分配，本地不应强制");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-adapter config`
Expected: FAIL —— `mavlink_addr must not be empty`（panic）。

- [ ] **Step 3: 改实现**

- `config.rs`：删 `device_id` / `devices` 字段与 `effective_device_id()` / `registration_devices()`；`apply_env()` 里删 `DEVICE_ID` / `DEVICES` 分支；`validate()` 里删掉 `mavlink_addr must not be empty` 这段（字段暂时保留、仅不再必填）。
- `fc/mod.rs` / `link/mod.rs`：把 `cfg.effective_device_id()` 的调用点改为常量 `"drone-001"` 兜底（Task 11 换成服务端下发值）。
- `adapter.toml.example`：删掉 `device_id` / `devices` 段，加注释"设备与端点由平台分配"。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-adapter` 与 `cargo check --workspace`
Expected: 全部通过（`fc`/`link` 的兜底常量已就位）。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-adapter adapter.toml.example
git commit -m "refactor(drone-adapter): 配置移除本地 device_id 与 devices"
```

---

### Task 11: adapter 注册倒置（先注册 → 收 assignment → 建链）

**Files:**
- Modify: `crates/rustcam-drone-adapter/src/link/mod.rs`
- Modify: `crates/rustcam-drone-adapter/src/fc/mod.rs`
- Modify: `crates/rustcam-drone-adapter/src/fc/addr.rs`
- Modify: `crates/rustcam-drone-adapter/src/config.rs`

**Interfaces:**
- Consumes: Task 2 的 `RegisterAck.assigned_drone_tags` / `DeviceAssignment`；Task 10 的瘦配置。
- Produces:
  - `fc::FlightController::connect(addr: &str, drone_tag: &str, cfg: &AdapterConfig) -> anyhow::Result<FlightController>`（端点与 drone_tag 由调用方传入）
  - `FlightController.drone_tag: String` 与 `FlightController::drone_tag(&self) -> &str`
  - `link::run` 流程：连接 service → `Register{adapter_id, protocol, version}` → 收到 `RegisterAck` → 逐条 assignment 调 `FlightController::connect` → 上报 `IdentityReport{drone_tag, uid}` → 心跳/遥测/事件按 drone_tag 上行
  - **实测 sysid 与分配期望值不符 → 告警**（不阻断）
  - `config.rs` 删除 `mavlink_addr` 字段、`MAVLINK_ADDR` 环境变量与 `validate()` 中的端点必填校验

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-adapter/src/link/mod.rs` 的 `mod tests` 内追加：

```rust
    #[tokio::test]
    async fn identity_report_carries_drone_tag_and_uid() {
        let (tx, mut rx) = mpsc::channel::<AdapterMessage>(8);
        let reporter = IdentityReporter::new("drone-a".into());
        reporter.report(&tx, "0011223344556677").await;
        match rx.try_recv().expect("应当上报").payload {
            Some(Payload::IdentityReport(report)) => {
                assert_eq!(report.identities[0].drone_tag, "drone-a");
                assert_eq!(report.identities[0].unique_id, "0011223344556677");
            }
            other => panic!("expected IdentityReport, got {other:?}"),
        }
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-adapter link`
Expected: FAIL —— `no field 'drone_tag'`。

- [ ] **Step 3: 改实现**

- `link/mod.rs`：`run_session` 改为「先注册、收到 RegisterAck 后再建链」；`handle_service_message` 的 `RegisterAck` 分支遍历 `ack.assignments`，逐条 `FlightController::connect(&assignment.mavlink_addr, &assignment.drone_tag, cfg)`；`IdentityReporter` 的设备标识改用 `drone_tag`。
- `fc/mod.rs`：`connect` 接受端点与 `drone_tag` 并存入结构体；新增 `drone_tag()`；收到 HEARTBEAT 时把实测 sysid 与分配的期望 sysid 比对，不符则 `tracing::warn!`。
- `config.rs`：删 `mavlink_addr` / `MAVLINK_ADDR` / 端点校验。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-adapter` 与 `cargo check --workspace`
Expected: 全部通过。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-adapter
git commit -m "feat(drone-adapter): 注册倒置，按服务端分配的端点建链"
```

---

### Task 12: adapter 多机连接池

**Files:**
- Modify: `crates/rustcam-drone-adapter/src/fc/mod.rs`
- Modify: `crates/rustcam-drone-adapter/src/fc/command.rs`
- Modify: `crates/rustcam-drone-adapter/src/fc/state.rs`
- Modify: `crates/rustcam-drone-adapter/src/link/mod.rs`

**Interfaces:**
- Consumes: Task 11 的 `FlightController`（单机）。
- Produces:
  - `fc::FcPool { links: HashMap<String, FlightController> }`（`Default` 可用）
  - `FcPool::execute(&self, drone_tag: &str, cmd: proto::Command) -> proto::CommandReply`（找不到目标 → 立即 `rejected`，不排队）
  - `FcPool::subscribe_telemetry() / subscribe_events()`：汇聚所有连接的遥测/事件（`Telemetry.drone_tag` / `FcEvent.drone_tag` 已带标签）
  - `FcPool::close()`、`FcPool::drone_tags() -> Vec<String>`

- [ ] **Step 1: 写失败测试**

在 `crates/rustcam-drone-adapter/src/fc/mod.rs` 的 `mod tests` 内追加：

```rust
    #[tokio::test]
    async fn pool_rejects_command_for_unknown_drone() {
        let pool = FcPool::default();
        let reply = pool
            .execute(
                "drone-missing",
                rustcam_drone_proto::common::Command {
                    cmd_id: "c-1".into(),
                    drone_tag: "drone-missing".into(),
                    r#type: "takeoff".into(),
                    ..Default::default()
                },
            )
            .await;
        assert_eq!(reply.status, "rejected");
        assert!(reply.message.contains("drone-missing"));
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test -p rustcam-drone-adapter fc::`
Expected: FAIL —— `cannot find type FcPool`。

- [ ] **Step 3: 改实现**

- `fc/mod.rs`：新增 `FcPool`（`Default` 可用，内部 `HashMap<String, FlightController>`）；`FlightController` 保留单机职责，构造时带 `drone_tag`。
- `fc/command.rs`：`execute` 的寻址参数改为该连接的实测 sysid（缺失时回落分配期望值）。
- `link/mod.rs`：用 `FcPool` 替换单个 `FlightController`；遥测/事件转发器订阅池的汇聚通道；指令下行按 `cmd.drone_tag` 路由。

- [ ] **Step 4: 跑测试确认通过**

Run: `cargo test -p rustcam-drone-adapter`
Expected: PASS。

- [ ] **Step 5: 提交**

```bash
git add crates/rustcam-drone-adapter
git commit -m "feat(drone-adapter): 飞控链路改多机连接池，按 drone_tag 路由"
```

---

### Task 13: SQL 迁移脚本（存量库）

**Files:**
- Create: `sql/migrate_remove_device_id.py`
- Create: `sql/verify_drone_schema.py`

**Interfaces:**
- Consumes: Task 7 的新表结构。
- Produces: 幂等迁移脚本，行为：
  1. `drone_device.drone_tag` 为空的行 → 生成 `drone-<uuid4>`
  2. 记录 `device_id → drone_tag` 对照
  3. `drone_device`：删主键 `drone_device_pkey` → 主键改 `drone_tag`；**删除 `device_id` 列**
  4. 新增唯一索引 `uq_drone_device_adapter_sysid`
  5. `drone_control_audit` / `drone_command_audit` / `drone_telemetry`：`device_id` 列改名 `drone_tag` 并按对照回填（孤儿历史生成一次性 `drone-<uuid4>`）
  6. 打印对照清单

- [ ] **Step 1: 写校验脚本**

创建 `sql/verify_drone_schema.py`（只读校验，参照 `sql/migrate_drone_link_identity.py` 的 `psycopg2` + `DATABASE_URL` 风格）：

```python
#!/usr/bin/env python3
"""校验无人机表结构已切换到 drone_tag（只读）。

用法：python sql/verify_drone_schema.py
"""

import os
import sys

import psycopg2

EXPECTED = {
    "drone_device": ["drone_tag", "adapter_id", "mavlink_addr", "mavlink_sysid", "mavlink_compid"],
    "drone_control_audit": ["drone_tag"],
    "drone_command_audit": ["drone_tag"],
}


def main() -> int:
    url = os.environ.get(
        "DATABASE_URL", "postgres://postgres:123456@192.168.1.7:5435/rustcam"
    )
    conn = psycopg2.connect(url)
    bad = []
    try:
        with conn.cursor() as cur:
            for table, cols in EXPECTED.items():
                cur.execute(
                    "select column_name from information_schema.columns where table_name = %s",
                    (table,),
                )
                have = {r[0] for r in cur.fetchall()}
                missing = [c for c in cols if c not in have]
                if missing:
                    bad.append(f"{table} 缺少列: {missing}")
                if "device_id" in have:
                    bad.append(f"{table} 仍存在 device_id 列")
    finally:
        conn.close()
    for line in bad:
        print("✗", line)
    if bad:
        return 1
    print("✓ 无人机表结构已切换到 drone_tag")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: 跑校验确认失败**

Run: `python sql/verify_drone_schema.py`
Expected: 非 0 退出并打印「仍存在 device_id 列」。

- [ ] **Step 3: 写迁移脚本**

创建 `sql/migrate_remove_device_id.py`：

```python
#!/usr/bin/env python3
"""移除无人机域的 device_id，主键改为 drone_tag（幂等）。

用法：python sql/migrate_remove_device_id.py
"""

import os
import sys
import uuid

import psycopg2

AUDIT_TABLES = ["drone_control_audit", "drone_command_audit", "drone_telemetry"]


def new_tag() -> str:
    return f"drone-{uuid.uuid4()}"


def has_column(cur, table: str, column: str) -> bool:
    cur.execute(
        "select 1 from information_schema.columns "
        "where table_name = %s and column_name = %s",
        (table, column),
    )
    return cur.fetchone() is not None


def main() -> int:
    url = os.environ.get(
        "DATABASE_URL", "postgres://postgres:123456@192.168.1.7:5435/rustcam"
    )
    conn = psycopg2.connect(url)
    mapping = {}
    try:
        with conn.cursor() as cur:
            # 1) 补齐 drone_device.drone_tag，并记录对照
            if has_column(cur, "drone_device", "device_id"):
                cur.execute("select device_id, coalesce(drone_tag, '') from drone_device")
                for device_id, drone_tag in cur.fetchall():
                    if not drone_tag:
                        drone_tag = new_tag()
                        cur.execute(
                            "update drone_device set drone_tag = %s where device_id = %s",
                            (drone_tag, device_id),
                        )
                    mapping[device_id] = drone_tag
                cur.execute("alter table drone_device drop constraint if exists drone_device_pkey")
                cur.execute("alter table drone_device drop column if exists device_id")
                cur.execute("alter table drone_device add primary key (drone_tag)")
            else:
                cur.execute("select coalesce(drone_tag, '') from drone_device")
                for (drone_tag,) in cur.fetchall():
                    if drone_tag:
                        mapping[drone_tag] = drone_tag

            # 2) 链路绑定唯一索引
            cur.execute(
                "create unique index if not exists uq_drone_device_adapter_sysid "
                "on drone_device (adapter_id, mavlink_sysid) "
                "where adapter_id <> '' and mavlink_sysid > 0"
            )

            # 3) 审计/遥测表改名并回填
            for table in AUDIT_TABLES:
                if not has_column(cur, table, "device_id"):
                    continue
                cur.execute(f"alter table {table} rename column device_id to drone_tag")
                cur.execute(f"select drone_tag from {table}")
                legacy = [r[0] for r in cur.fetchall() if r[0]]
                for value in legacy:
                    if value not in mapping:
                        mapping[value] = new_tag()
                for old, new in mapping.items():
                    cur.execute(
                        f"update {table} set drone_tag = %s where drone_tag = %s",
                        (new, old),
                    )
        conn.commit()
    finally:
        conn.close()

    print("device_id → drone_tag 对照：")
    for old, new in sorted(mapping.items()):
        print(f"  {old} -> {new}")
    print("无人机域 device_id 已移除")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: 跑校验确认通过**

Run: `python sql/migrate_remove_device_id.py` 然后 `python sql/verify_drone_schema.py`
Expected: 后者退出码 0 并打印「✓ 无人机表结构已切换到 drone_tag」。

- [ ] **Step 5: 提交**

```bash
git add sql
git commit -m "feat(sql): 新增无人机域 device_id 移除迁移脚本"
```

---

### Task 14: 前端改用 drone_tag

**Files:**
- Modify: `frontend/src/api/drone.js`
- Modify: `frontend/src/stores/droneStore.js`
- Modify: `frontend/src/views/drone/DroneList.vue`
- Modify: `frontend/src/views/drone/DroneControl.vue`
- Modify: `frontend/src/views/drone/DroneLive.vue`
- Modify: `frontend/src/views/drone/RoutePlanner.vue`
- Modify: `frontend/src/views/drone/FenceManager.vue`
- Modify: `frontend/src/views/drone/DroneAlerts.vue`
- Modify: `frontend/src/views/drone/DroneReplay.vue`
- Modify: `frontend/src/components/drone/Map2D.vue`
- Modify: `frontend/src/components/drone/CesiumGlobe.vue`

**Interfaces:**
- Consumes: Task 8/9 的 REST 契约（路径与查询参数 `drone_tag`，响应体 `drone_tag`）。
- Produces: 前端无人机域不再出现 `device_id`；`getDrones()` 返回项以 `drone_tag` 为 key；档案表单不再提交 `device_id`（改由平台生成 `drone_tag`，只读展示）；新增「实测唯一标识」展示（取遥测 `unique_id`）。

- [ ] **Step 1: 写失败测试**

在 `frontend/tests/` 新增 `drone-drone-tag.spec.js`：

```javascript
import { test, expect } from '@playwright/test'

test('无人机列表以 drone_tag 作为标识', async ({ page }) => {
  await page.goto('/drone/drones')
  await expect(page.locator('[data-testid="drone-tag"]').first()).toHaveText(/^drone-/)
})
```

并在 `DroneList.vue` 的 `drone-id` 元素（当前渲染 `row.drone_tag || row.device_id`）加 `data-testid="drone-tag"`。

- [ ] **Step 2: 跑测试确认失败**

Run: `cd frontend && npm run test -- drone-drone-tag.spec.js`
Expected: FAIL —— 元素不存在或文本不是 `drone-` 开头。

- [ ] **Step 3: 改实现**

- `api/drone.js`：所有 `device_id` 参数改 `drone_tag`（含 query 参数 `?drone_tag=`）。
- `droneStore.js`：state 的 drone 集合以 `drone_tag` 为键；`telemetry` map 的键改 `drone_tag`。
- 7 个视图 + 2 个组件：`:key`/`:value`/选择项/查询参数全改 `drone_tag`；`DroneList.vue` 表单删除 `device_id` 输入框，新增只读 `drone_tag` 展示与「实测唯一标识」展示（遥测里的 `unique_id`）。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd frontend && npm run build` 与 `npm run test -- drone-drone-tag.spec.js`
Expected: 构建成功、用例通过。

- [ ] **Step 5: 提交**

```bash
git add frontend
git commit -m "refactor(frontend): 移动眼前端统一使用 drone_tag"
```

---

### Task 15: 文档同步

**Files:**
- Modify: `drone.md`
- Modify: `public/api.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: Task 2/9/13/14 的最终契约。
- Produces: 文档与实现一致（标识模型、proto 契约、表结构、接口清单、部署与快速接入说明）。

- [ ] **Step 1: 写失败测试**

以 grep 断言代替单测：

Run: `grep -n "device_id" drone.md public/api.md`
Expected: 命中行全部落在**摄像机域**语义（GB28181 / ONVIF / 摄像机章节），无人机章节不得命中。

- [ ] **Step 2: 跑测试确认失败**

Run: 同上
Expected: 无人机相关章节仍有 `device_id` 命中。

- [ ] **Step 3: 改文档**

- `drone.md`：§5.8 契约、§9.1 表结构、§12.3 接口清单、§12.4 身份核验全部改 `drone_tag`；新增一小节说明标识模型（`drone_tag` / `unique_id` / `(adapter_id, mavlink_sysid)` 三者分工）与「引导模式取消」的运维影响。
- `public/api.md`：无人机接口清单的路径与字段改 `drone_tag`。
- `README.md`：移动眼章节的快速接入补充「设备与端点由平台分配，adapter.toml 不再配 device_id」。

- [ ] **Step 4: 跑测试确认通过**

Run: `grep -n "device_id" drone.md public/api.md`
Expected: 无人机章节无命中。

- [ ] **Step 5: 提交**

```bash
git add drone.md public/api.md README.md
git commit -m "docs(drone): 标识模型与接口清单同步为 drone_tag"
```

---

### Task 16: 端到端双机验证

**Files:**
- Create: `testenv/verify-drone-tag.mjs`

**Interfaces:**
- Consumes: 全部前置任务的产物。
- Produces: 可重复执行的验证脚本，覆盖：两台 sim（不同 sysid）→ 一台 adapter → drone-service → server；断言两台遥测分别归属各自 `drone_tag`、指令各自路由、唯一标识不匹配的一台被拒。

- [ ] **Step 1: 写脚本**

创建 `testenv/verify-drone-tag.mjs`：

```javascript
// 双机链路验证：两台 sim（不同 sysid）经一个 adapter 接入，
// 断言遥测归属、指令路由、唯一标识核验。
// 用法：node testenv/verify-drone-tag.mjs   （需先起好 4 个进程）
const BASE = process.env.RUSTCAM_BASE || 'http://127.0.0.1:8081'

async function api(method, path, body, token) {
  const headers = { 'Content-Type': 'application/json' }
  if (token) headers.Authorization = `Bearer ${token}`
  const res = await fetch(`${BASE}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  })
  const json = await res.json().catch(() => ({ code: res.status, message: res.statusText }))
  return json
}

async function login() {
  const r = await api('POST', '/api/v1/auth/login', {
    username: process.env.RUSTCAM_USER || 'admin',
    password: process.env.RUSTCAM_PASS || 'admin123',
  })
  if (!r?.data?.access_token) throw new Error(`登录失败: ${JSON.stringify(r)}`)
  return r.data.access_token
}

async function waitForDrones(token, count, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    const r = await api('GET', '/api/v1/drone/drones', null, token)
    const list = Array.isArray(r?.data) ? r.data : []
    const online = list.filter((d) => d.online)
    if (online.length >= count) return online
    await new Promise((res) => setTimeout(res, 1000))
  }
  throw new Error(`等待 ${count} 台在线无人机超时`)
}

async function waitCommand(token, cmdId, timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs
  let last = null
  while (Date.now() < deadline) {
    const r = await api('GET', `/api/v1/drone/commands/${cmdId}`, null, token)
    last = r?.data
    if (last && ['accepted', 'failed', 'timeout', 'rejected'].includes(last.status)) return last
    await new Promise((res) => setTimeout(res, 500))
  }
  return last
}

const results = []
function check(name, ok, detail) {
  results.push({ name, ok, detail })
  console.log(`${ok ? '[PASS]' : '[FAIL]'} ${name}  ${detail}`)
}

const token = await login()
const drones = await waitForDrones(token, 2)
check('两台无人机在线', drones.length >= 2, `online=${drones.length}`)

const tags = drones.map((d) => d.drone_tag)
check('两台 drone_tag 互不相同', new Set(tags).size === tags.length, tags.join(','))

const uids = drones.map((d) => d.last_telemetry?.unique_id || '')
check('两台实测唯一标识互不相同', new Set(uids).size === uids.length && uids.every(Boolean), uids.join(','))

// 各自申请控制权并下发 takeoff，验证指令按 drone_tag 路由
for (const d of drones) {
  const acq = await api('POST', '/api/v1/drone/control/acquire', {
    drone_tag: d.drone_tag,
    control_type: 'flight',
    priority: 10,
    lease_seconds: 60,
  }, token)
  check(`申请控制权 ${d.drone_tag}`, acq?.data?.granted === true, JSON.stringify(acq?.data || acq?.message))

  const cmd = await api('POST', `/api/v1/drone/${encodeURIComponent(d.drone_tag)}/command`, {
    kind: 'takeoff',
    params: { alt: '30' },
  }, token)
  const final = await waitCommand(token, cmd?.data?.cmd_id)
  check(`指令闭环 ${d.drone_tag}`, final?.status === 'accepted', `status=${final?.status} msg=${final?.message}`)
  await api('POST', '/api/v1/drone/control/release', {
    drone_tag: d.drone_tag,
    control_type: 'flight',
  }, token)
}

// 唯一标识不匹配的一台应被拒绝
const victim = drones[0]
const profile = await api('GET', `/api/v1/drone/drones/${encodeURIComponent(victim.drone_tag)}`, null, token)
const body = { ...profile.data, unique_id: 'ffffffffffffffff' }
const upd = await api('PUT', `/api/v1/drone/drones/${encodeURIComponent(victim.drone_tag)}`, body, token)
check('登记不匹配的唯一标识', upd?.code === 200, JSON.stringify(upd?.message || upd?.code))

const bad = await api('POST', `/api/v1/drone/${encodeURIComponent(victim.drone_tag)}/command`, {
  kind: 'rtl',
}, token)
const rejected = bad?.data?.status === 'rejected' || (bad?.message || '').includes('mismatch')
check('唯一标识不匹配 → 拒绝下发', rejected, `status=${bad?.data?.status} msg=${bad?.message}`)

const failed = results.filter((r) => !r.ok).length
console.log(`\n=== 结果: PASS=${results.length - failed} FAIL=${failed} ===`)
process.exit(failed ? 1 : 0)
```

- [ ] **Step 2: 跑脚本确认失败**

Run（先起 4 个进程 + 两台 sim）：`node testenv/verify-drone-tag.mjs`
Expected: 失败（此时字段/路由尚未全部切换）。

- [ ] **Step 3: 起双机环境跑通**

Run：

```bash
cargo run -p rustcam-drone-service
cargo run -p rustcam-server
cargo run -p rustcam-drone-sim -- --uid 0011223344556677 --sysid 1 --addr udpout:127.0.0.1:14540
cargo run -p rustcam-drone-sim -- --uid 0011223344556678 --sysid 2 --addr udpout:127.0.0.1:14541
cargo run -p rustcam-drone-adapter
node testenv/verify-drone-tag.mjs
```

Expected: 脚本全部断言通过（两台遥测归属正确、指令各自路由、唯一标识不匹配的一台被拒）。

> 前置：平台档案里要为两台机体各录一行 `drone_device`（`adapter_id` 相同、`mavlink_sysid` 分别为 1/2、`mavlink_addr` 分别为上面两个端点、`unique_id` 分别为上面的 uid），否则服务端不会下发 assignment。

- [ ] **Step 4: 全量质量门**

Run: `cargo fmt --all` → `cargo test --workspace --no-fail-fast` → `cargo clippy -p rustcam-drone-core -p rustcam-drone-proto -p rustcam-drone-service -p rustcam-drone-adapter --all-targets -- -D warnings`
Expected: fmt 无改动；测试除已知的 `test_parse_xml_field_with_special_chars_in_content` 外全绿；clippy 无输出。

- [ ] **Step 5: 提交**

```bash
git add testenv/verify-drone-tag.mjs
git commit -m "test(drone): 双机链路与唯一标识核验端到端脚本"
```
