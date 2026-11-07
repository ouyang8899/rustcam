# 设计规格：彻底移除 `device_id`，统一以 `drone_tag` 为主键

- 日期：2026-11-07
- 状态：待评审
- 范围：`rustcam-drone-*`、`rustcam-media`（drone 网关）、前端 `移动眼`、SQL 迁移、`drone.md` / `public/api.md`

## 1. 背景与目标

当前平台里同一台无人机有三个字符串标识，职责重叠、层级混乱：

| 标识 | 产生方 | 现状问题 |
|---|---|---|
| `device_id` | adapter.toml 人填（"与 adapter 配置一致"） | ① 人填字符串 → 两个地面站都叫 `drone-001` 就撞车；② 作为 `drone_device` 主键、Redis 独占键、租约键、影子键 → 撞车即互抢设备；③ `drone_tag` 已是平台唯一标识，二者指向同一台机体 |
| `drone_tag` | 平台生成 `drone-<uuid4>` | 只用于 `drone_camera` 外键与摄像头关联子路由，主键角色空缺 |
| `unique_id` | 飞控 `AUTOPILOT_VERSION.uid` | 物理身份锚点，档案登记 vs 实测比对 |

### 目标

1. **删除 `device_id` 这一概念**，平台唯一主键为 `drone_tag`。
2. 链路绑定改由 **`(adapter_id, mavlink_sysid)`** 表达：飞控帧只有 sysid/compid，平台靠它把遥测落到具体机体；sysid ∈ 1..255，故一个 adapter 最多管理 255 台。
3. 端点、签名密钥、期望 sysid/compid 全部由服务端按 `adapter_id` 分配（drone.md §12.4 步骤 5 的"先注册后连飞控 + 多机连接池"）。
4. 一次性切换，不保留过渡期双键。

### 非目标

- 不改 `unique_id` 的核验语义（L3 已实现且已验证）。
- 不实现 MAVLink 2 签名（L2）与 mTLS。
- 不改 ZLMediaKit / RTSP / GB28181 等非无人机链路。

## 2. 标识模型（最终）

| 概念 | 取值 | 产生方 | 角色 | 出现位置 |
|---|---|---|---|---|
| `drone_tag` | `drone-<uuid4>`（保留连字符，与现生成方式一致） | 平台（drone-service 建档时生成，DB 唯一索引） | 平台主键、对外标识 | DB 主键、REST 路径、前端 key、Redis 租约/影子/独占键、proto 字段、审计表 |
| `unique_id` | `AUTOPILOT_VERSION.uid`（小写 hex） | 飞控 | 物理身份核验锚点 | 档案登记值 / 实测值比对 |
| `(adapter_id, mavlink_sysid)` | — | 平台建档时指定 + 飞控实测 | 链路绑定：这条链路是谁 | `drone_device` 唯一索引 `(adapter_id, mavlink_sysid)` |
| `mavlink_addr` | 端点串 | 平台建档时录入 | adapter 连哪条链路 | `drone_device.mavlink_addr` → `DeviceAssignment` |
| `name` / `model` | 文本 | 人填 | 可读名 | 档案 |

**删除**：`device_id`（字段、配置项、DB 列、API 参数、前端 key、文档）。

### 2.1 `mavlink_sysid` 的语义（本次明确）

- `drone_device.mavlink_sysid`：该机在**其归属 adapter 的链路上**的 system id（1..255）。
- 唯一约束：`(adapter_id, mavlink_sysid)` 唯一（`adapter_id` 非空且 `mavlink_sysid > 0` 时生效）→ 同一 adapter 内两台机体不得同 sysid。
- adapter 建链后从 HEARTBEAT 学**实测** sysid：
  - 与分配的期望值一致 → 正常。
  - 不一致 → **告警**（连错机 / 端点配错），不阻断（可能飞控固件换了 sysid）。
- 出站指令寻址：优先用实测 sysid；尚未收到 HEARTBEAT 时回落到分配的期望值。

## 3. 协议变更（`rustcam-drone-proto`）

字段号不变、字段名 `device_id` → `drone_tag`（一次性切换，不保 wire 兼容）。

### 3.1 `common.proto`

| 消息 | 变更 |
|---|---|
| `Telemetry` | `device_id`(1) → `drone_tag`(1)；`unique_id`(30) 保留 |
| `Command` | `device_id`(2) → `drone_tag`(2) |
| `CommandReply` | `device_id`(2) → `drone_tag`(2) |
| `DeviceEvent` | `device_id`(2) → `drone_tag`(2) |

### 3.2 `adapter_link.proto`

| 消息 | 变更 |
|---|---|
| `Register` | **删除** `devices`(4)（设备由服务端分配，adapter 不自报）；`identities`(5) → 删除（改由 `IdentityReport` 承载） |
| `DeviceIdentity` | `device_id`(1) → `drone_tag`(1)；`unique_id`(2) 保留 |
| `IdentityReport` | 字段不变（`repeated DeviceIdentity identities`） |
| `DeviceAssignment` | `device_id`(1) → `drone_tag`(1)；`mavlink_addr`(2)、`signing_secret`(3)、`allow_unsigned`(4)、`mavlink_version`(5)、`mavlink_sysid`(6)、`mavlink_compid`(7) 保留 |
| `RegisterAck` | `assigned_devices`(3) → `assigned_drone_tags`(3)；`assignments`(5) 保留 |
| `ControlState` | `device_id`(1) → `drone_tag`(1) |

### 3.3 `control.proto`

全部 `device_id` → `drone_tag`，`repeated device_ids` → `repeated drone_tags`：

`ControlRequest` / `ControlState` / `AcquireRequest` / `PreemptRequest` / `ReleaseRequest` / `RenewRequest` / `ForceReleaseRequest` / `CommandStatus` / `AlertListRequest` / `Alert` / `AuditListRequest` / `Audit` / `Route` / `RouteListRequest` / `RouteExecuteRequest` / `Geofence` / `GeofenceListRequest` / `GeofenceUpsertRequest` / `TelemetryHistoryRequest` / `DroneProfile`（`device_id`(1) → `drone_tag`(1)，`drone_tag`(21) **删除**，避免同义两字段）。

> `DroneProfile` 现状同时有 `device_id`(1) 与 `drone_tag`(21)，二者同义 → 合并为一个 `drone_tag`(1)。

## 4. 服务端变更（`rustcam-drone-service`）

### 4.1 内存状态

| 结构 | 变更 |
|---|---|
| `state.devices` | 键 `device_id` → `drone_tag` |
| `state.profiles` | 键 → `drone_tag` |
| `state.device_to_adapter` | 键 → `drone_tag`（值仍为 `adapter_id`） |
| `DeviceEntry` / `DeviceProfileRecord` / `DroneProfileRecord` | 字段 `device_id` → `drone_tag` |
| 租约 / 影子 / 审计 / 告警 | 全部以 `drone_tag` 为键 |

### 4.2 分配与注册（`registry.rs`）

1. `load_device_links(adapter_id)` → 改查 `drone_device WHERE adapter_id = ?`，返回行含 `drone_tag` / `mavlink_addr` / `mavlink_sysid` / `mavlink_compid` / 密钥。
2. 分配集合 = 该 adapter 名下的 `drone_tag` 列表；`RegisterAck.assigned_drone_tags` + `assignments`。
3. 独占判定：`state.device_to_adapter[drone_tag]` 已属其他 adapter → 不分配（逻辑不变，仅换键）。
4. Redis 独占键：`adapter:device:{drone_tag}`（值 = `adapter_id`）——形状不变，仅换键名（`drone_tag` 全局唯一，单键即可）。
5. **注册阶段不再做唯一标识校验**：此时 adapter 尚未连飞控，拿不到 uid。唯一标识核验统一收敛到两处——建链后的 `IdentityReport`（不符即撤销分配）与指令下发前的 `check_identity`。

### 4.3 建档（`admin.rs`）

- `upsert_profile` 的入参从 `device_id` 改为 `drone_tag`：
  - 传空 → 生成 `drone-<uuid4>`（沿用 `resolve_drone_tag` 逻辑）。
  - 传已存在的 `drone_tag` → 更新该档案（保持不可变）。
- 新增校验：`(adapter_id, mavlink_sysid)` 唯一 —— 与其他档案冲突 → 拒绝（`already_exists`）。
- `mavlink_sysid` / `mavlink_compid`：`0` = 未指定 → 保留已有值 / 默认 `1`（本次已实现，见 §8）。

### 4.4 指令与控制权

- `commands.rs`：`check_identity` 按 `drone_tag` 取档案与实测值；`cmd.drone_tag` 全链路透传。
- `control.rs`：租约键 `control:{drone_tag}:{type}`。
- 指令下发：按 `drone_tag` → 查 `device_to_adapter` → 找到 adapter 的该 assignment → 下发（附带该机的 sysid/compid 供寻址）。

## 5. 适配器变更（`rustcam-drone-adapter`）

### 5.1 配置（`config.rs`）

**删除**：`device_id`、`devices`、`mavlink_addr`（端点改为服务端分配）。
**保留**：`adapter_id`、`protocol`、`service_addr`、`autopilot`、`source_system`、`source_component`、`telemetry_hz`、`heartbeat_*`、`command_timeout_*`、`reconnect_*`、`buffer_size`。

### 5.2 启动顺序倒置（`link/mod.rs`）

```
1. 连接 drone-service（gRPC）
2. Register{ adapter_id, protocol, version }        // 不带设备列表
3. 收到 RegisterAck{ assigned_drone_tags, assignments }
4. 对每个 assignment 建一条飞控连接（一机一连）
5. 每条连接：连上 → 读 HEARTBEAT → 学实测 sysid/compid → 与期望值比对（不符告警）
6. 上报 IdentityReport{ drone_tag, uid }
7. 心跳 / 遥测 / 事件按 drone_tag 打标上行
```

- 某台飞控连不上 → 其余照常；该台进入重连队列（指数退避）。
- 全部 assignment 为空 → 无飞控可连，日志明确提示"该 adapter 在平台没有绑定无人机"。
- 断线重连：重新注册 → 重新拿 assignment → 重建连接（assignment 以服务端为准）。

### 5.3 多机连接池（`fc/`）

现状 `FlightController` 是单连接（一个 conn + 一份 state）。改为：

```
FcPool
├── connections: HashMap<drone_tag, FcLink>
└── FcLink（原 FlightController 的职责，单机一份）
    ├── conn / shared(state, pending, target_system, target_component)
    ├── telemetry_tx / event_tx（带 drone_tag 标签）
    └── close()
```

- 遥测 / 事件：每条连接独立产生，汇聚时打上 `drone_tag`。
- 指令：`FcPool::execute(drone_tag, cmd)` → 找到对应连接执行；找不到 → 立即失败（不回执排队）。
- 幂等台账（`CommandLedger`）按 `cmd_id` 全局去重，跨连接共用。

## 6. 网关 / API 变更（`rustcam-media`）

### 6.1 DTO

| DTO | 变更 |
|---|---|
| `DroneDto` | 删除 `device_id`；保留 `drone_tag`、`name`、`model`、`unique_id`、`mavlink_addr`、`signing_*`、`mavlink_*`、`last_telemetry` 等 |
| `TelemetryDto` | `device_id` → `drone_tag`（已含 `unique_id`，本次已实现，见 §8） |
| `CommandDto` / `CommandRequest` | `device_id` → `drone_tag` |
| `ControlDto` / `ControlRequest` | `device_id` → `drone_tag` |
| `RouteDto` / `FenceDto` | `device_id` → `drone_tag`，`device_ids` → `drone_tags` |
| `AlertDto` / `AuditDto` / `DroneEventDto` | `device_id` → `drone_tag` |

### 6.2 路由

| 现状 | 变更 |
|---|---|
| `GET/POST /api/v1/drone/drones` | 不变 |
| `GET/PUT/DELETE /api/v1/drone/drones/{device_id}` | → `{drone_tag}` |
| `GET /api/v1/drone/devices/{device_id}` | → `{drone_tag}`（保留该别名路径，仅换参数名） |
| `POST /api/v1/drone/{device_id}/command` | → `{drone_tag}` |
| `GET /api/v1/drone/{device_id}/telemetry` | → `{drone_tag}` |
| `GET /api/v1/drone/routes?device_id=` | → `?drone_tag=` |
| `GET /api/v1/drone/fences?device_id=` | → `?drone_tag=` |
| `GET /api/v1/drone/audits?device_id=` | → `?drone_tag=` |
| `POST /api/v1/drone/control/*` | 请求体 `device_id` → `drone_tag` |

`permissions.rs` 的路径→权限映射表同步改名（路径形状不变，权限集合不变）。

### 6.3 无人机档案表单（前端）

- 删除 `device_id` 输入框；`drone_tag` 只读展示（保存后生成）。
- 保留并强化「唯一标识」输入框：提示"填飞控 `AUTOPILOT_VERSION.uid`（小写 hex）"。
- 新增展示「实测唯一标识」= 遥测里的 `unique_id`（便于运维粘贴登记值）。
- 新增「MAVLink 端点」「期望 sysid/compid」输入框（原本已有 sysid/compid，但**从未生效**——本次起真正生效）。

## 7. 数据迁移

新增 `sql/migrate_remove_device_id.py`（幂等，可重复执行）：

1. `drone_device`：
   - `drone_tag` 为空的行 → 填 `drone-<uuid4>`。
   - 删除主键约束 `drone_device_pkey` → 新主键 `drone_tag`。
   - **删除 `device_id` 列**。
   - 新增唯一索引 `uq_drone_device_adapter_sysid ON drone_device (adapter_id, mavlink_sysid) WHERE adapter_id <> '' AND mavlink_sysid > 0`。
2. `drone_control_audit` / `drone_command_audit` / `drone_telemetry`：
   - `device_id` 列 → 改名 `drone_tag`，按迁移时的 `device_id → drone_tag` 对照回填。
   - 对照来源：能匹配到 `drone_device` 行的 `device_id` → 取该行的 `drone_tag`（历史连续性保持）；
     匹配不到（孤儿历史）→ 生成一次性 `drone-<uuid4>` 兜底，历史记录保留但不再关联到现存档案。
3. 迁移脚本输出对照清单（`device_id → drone_tag`）到标准输出，供人工排查；**不保留** `legacy_device_id` 列。
4. `crates/rustcam-drone-service/migrations.sql` 同步为新结构（新建库直接得到新表）。
5. `sql/schema.sql` 若含无人机表定义则同步。

> 建档数据准备：迁移后需为每台在飞无人机补 `adapter_id` + `mavlink_sysid` + `mavlink_addr`，否则该 adapter 拿不到 assignment。

## 8. 本次已落地的前置修复（同批保留）

1. `drone-service/src/admin.rs`：`mavlink_sysid`/`mavlink_compid` 为 `0`（= 未指定，查询接口不返回该字段）时**保留已有值 / 取默认值**，不再以 400 拒绝——修掉"档案无法编辑"。
2. `rustcam-media` `TelemetryDto` 增加 `unique_id` → 实测唯一标识透出到 REST，便于与档案登记值比对。
3. `drone.toml` / `rustcam-media/src/drone_gateway/ws.rs` / `rustcam-drone-service/src/grpc/admin.rs`：GBK → UTF-8（原文件非法 UTF-8，导致 `rustcam-media` / `drone-service` 编译失败）。

## 9. 兼容与发布

- **一次性切换**：adapter / drone-service / rustcam-server / 前端必须同版本发布；老 adapter 注册后拿不到 assignment（`Register` 协议已变），表现为"注册成功但无链路"，日志需明确提示版本不匹配。
- 不保留双键、不保留过渡期兼容分支。
- 回滚策略：代码回滚 + 数据库需反向迁移（迁移脚本需提供 `--rollback` 或人工按对照清单还原）。

## 10. 测试策略

| 层级 | 用例 |
|---|---|
| drone-core | `keys.*` 键名以 `drone_tag` 拼装；租约/影子键不含 `device_id` |
| drone-service | 分配按 `adapter_id` 查库返回 `drone_tag`；`(adapter_id, sysid)` 冲突拒绝；建档生成 `drone_tag` 且不可变；`check_identity` 按 `drone_tag` 判定；`IdentityReport` 不符撤销分配 |
| drone-adapter | 注册→收 assignment→建链→读 sysid→不符告警；多连接并行遥测按 `drone_tag` 归属；下行指令按 `drone_tag` 路由到对应连接；未连接目标 → 立即失败 |
| rustcam-media | DTO 往返保 `drone_tag`；REST 路径 `{drone_tag}` 命中权限映射；`drone_tag` 缺失 → 400 |
| 前端 | 移动眼各页用 `drone_tag` 作为 key 与选择值；档案表单不再提交 `device_id` |
| 端到端 | 4 进程实跑：sim（2 台不同 sysid）→ adapter → drone-service → server：两台机遥测分别归属、指令各自路由、唯一标识不匹配的一台被拒 |

## 11. 风险

| 风险 | 缓解 |
|---|---|
| 多机连接池改造量大（adapter 飞控层重构） | 分阶段：先协议 + 服务端 + 网关（drone_tag 化），再 adapter 连接池；每阶段保持可编译可测 |
| 存量数据迁移失败 | 迁移脚本幂等 + 迁移前备份；对照清单人工核对 |
| 一个 adapter 接多台后，遥测归属错乱 | 归属依据"连接 ↔ assignment"，不依赖 sysid；实测 sysid 仅作一致性校验 |
| sysid 冲突（同 adapter 两台同号） | DB 唯一索引拦截 |
| 老 adapter / 老前端失配 | 一次性同版本发布；日志明确提示 |

## 12. 实施顺序（供计划参考）

1. `rustcam-drone-core`：`keys` 与共享类型改名
2. `rustcam-drone-proto`：proto 字段改名 + 生成代码
3. `rustcam-drone-service`：状态键、注册分配、建档、指令、控制权、审计、DB 层
4. `rustcam-media`：DTO、路由、权限映射、网关调用
5. `rustcam-drone-adapter`：配置瘦身 → 注册倒置 → 多机连接池
6. SQL 迁移脚本 + `migrations.sql` + `schema.sql`
7. 前端：api / store / 7 个视图 / 档案表单
8. 文档：`drone.md`、`public/api.md`、`README.md`
9. 端到端实跑验证（双机）
