# RustCam-Media 无人机接入设计

> 本文档由《下面给一套完整、可落地的架构与代码结构。原则：现有 Rust SIP 视频服务器.md》整理而来，去除了讨论过程与备选方案，保留最终结论、契约、目录结构、关键实现与落地步骤。
>
> 设计基线：当前仓库 `RustCam-Media`（单 crate + Vue 3 前端 + PostgreSQL/rbatis + Redis + Casbin + MapLibre/terra-draw）。
>
> 核心原则：**现有 SIP/视频/ONVIF/RTSP 能力不动；无人机能力以增量模块和独立进程接入；控制面与协议面分离。**

### 关键决策速览（与现有代码库核对后的结论）

| 议题 | 结论 |
|---|---|
| 仓库形态 | 最小 workspace：`rustcam-media`（lib）、`rustcam-server`（bin）、`rustcam-drone-proto`、`rustcam-drone-core`、`rustcam-drone-service`（bin）、`rustcam-drone-adapter`（bin） |
| 进程划分 | **三个独立进程**：`rustcam-server`（SIP/媒体/API/WS 网关）、`rustcam-drone-service`（无人机控制面）、`rustcam-drone-adapter`（协议适配）；drone 业务不嵌入主服务进程 |
| 共享边界 | 只共享 `rustcam-drone-proto` + `rustcam-drone-core`；不抽 `EventBus`，不建 `rustcam-infra`，infra 各进程各写薄封装 |
| drone-service | 不对外提供 HTTP；只暴露内网 gRPC；业务 API 的唯一对外入口仍是 rustcam-server 的 `/api/v1/drone/*` |
| REST 鉴权 | 复用 `jwt_auth_layer`，drone 路由必须并入 `protected_routes` |
| WS 鉴权 | 浏览器无法带 header，用 `POST /api/v1/drone/ws-ticket` 换一次性票据 |
| WS 消息包 | 沿用现有 `{msg_type, data, timestamp}`，不新造协议 |
| Redis | drone-service 自建客户端（Lua/Streams/NX），不复用 rustcam-media 的 `RedisStore` |
| 建表 | drone-service 启动时执行自带幂等 `migrations.sql`，主服务的 `ensure_schema` 不感知 drone 表 |
| 指标 | 两个进程各自暴露 `/metrics`；drone 指标实现放在 `rustcam-drone-core` 内，不引入独立 Prometheus registry |
| 围栏 | 仓库已启用 PostGIS，但 MVP 仍以 `geojson text` 为权威源 |
| 视频关联 | 无人机相机走现有 RTSP/ZLMediaKit 链路，`drone_device.device_tag/channel_tag` 只做业务关联 |

---

## 1. 结论摘要

| 项目 | 最终选择 |
|---|---|
| 仓库形态 | 最小 workspace：`rustcam-media`（原 lib）、`rustcam-server`（原 bin）、`rustcam-drone-proto`、`rustcam-drone-core`、`rustcam-drone-service`、`rustcam-drone-adapter` |
| 共享边界 | 只共享 `rustcam-drone-proto` + `rustcam-drone-core`；不抽 EventBus，不建 `rustcam-infra` |
| 无人机业务位置 | **独立进程** `rustcam-drone-service`（第一天就与主服务分开，不嵌入 `rustcam-media`） |
| 服务间通信 | gRPC bidi：adapter 启动即主动连 service，首帧 `Register`；rustcam-server 通过 gRPC 调 service |
| 控制权仲裁 | Redis Hash + TTL + Lua（`EVALSHA`），实现 acquire/release/renew/preempt |
| 遥测主路径 | adapter → gRPC bidi → drone-service → 去重/影子/广播 → EventService 流 → rustcam-server → WebSocket |
| 异步与历史 | Redis Streams 做解耦与归档，TimescaleDB 存轨迹历史（可选，后置） |
| 围栏存储 | MVP：`geojson text` + `geo` crate 内存判断；规模化再上 PostGIS |
| 前端 | 复用现有 Vue 3 + Vite + Pinia + Element Plus；围栏用 terra-draw；3D 新增 Cesium |
| 前端语言 | JavaScript + JSDoc + `// @ts-check`，消息做运行时校验，不引入 TS 构建链 |
| 权限 | rustcam-server 侧复用现有 JWT + Casbin；drone-service 信任内网调用方，不重复鉴权 |
| 视频 | 无人机相机走现有 RTSP/GB28181/ZLMediaKit 链路，drone 只负责云台/飞行控制权 |

一句话：**SIP 主服务保持稳定，drone 拆成独立的控制面进程与适配进程；同步控制走 gRPC，异步事件走 Redis Streams，控制权走 Redis Lua，前端在现有 Vue 工程内新增 `/drone/*`，业务入口仍是 rustcam-server。**

---

## 2. 现状基线

### 2.1 仓库现状

| 现状 | 对本设计的意义 |
|---|---|
| 根 `Cargo.toml` 为单 package，`[lib] rustcam_media` + `[[bin]] rustcam-media` | 可无损拆成 workspace，crate 名不变，`use rustcam_media::` 无需修改 |
| `src/` 已分层：`api/ application/ auth/ common/ context/ domain/ infrastructure/ media/ monitoring/ protocol/ transport/` | 原 lib 整体迁移，不按业务拆细 crate，避免循环依赖 |
| `src/adapter/` 是现有协议适配层 | 与新 `rustcam-drone-adapter` 不是同一概念，命名上需区分 |
| `src/api/` 已有 `http/ state.rs websocket.rs` | REST 走 `/api/v1/drone/*`，挂进 `protected_routes`；WS 走 `/ws/drone/*`，需单独鉴权 |
| 已有 `tokio 1.54`、`axum 0.8.9`（ws）、`redis 1.7.1`（tokio-comp/connection-manager）、`rbatis 4.9.8` + `rbdc-pg` | 版本与能力基线，drone-service 用同版本依赖自建 Redis/PG 客户端（不复用主服务实现） |
| 已有 `jsonwebtoken 11`、`casbin 2`、`argon2` | 鉴权与权限在 rustcam-server 侧直接复用；drone-service 不做 JWT/Casbin |
| 已有 `dashmap 6.2`、`parking_lot 0.13` | adapter 连接表、设备映射、内存缓存 |
| 前端 Vue 3 + Vite 5 + Pinia + Vue Router + Element Plus | 新增 `/drone/*` 路由与 Pinia store |
| 前端已有 `maplibre-gl 6.10`、`terra-draw 1.35`、`terra-draw-maplibre-gl-adapter 1.4` | 2D 地图与围栏绘制无需新增依赖 |
| 前端已有 `echarts`、`supercluster` | 高度曲线、多机聚合可直接用 |
| 已有 PostgreSQL（`config.toml` 指向 pg） | 可加 PostGIS/TimescaleDB 扩展；MVP 先用普通表 |
| 已有 Redis（`config.toml` 指向 redis） | 控制权、注册表、Streams、幂等键 |
| `sql/` 为散装 SQL/Python 迁移脚本，启动时不会自动执行 | drone 表由 `rustcam-drone-service` 自带的 `migrations.sql` 负责，与主服务 schema 完全隔离 |
| `config.toml` 已有 `[server] [database] [redis] [jwt]` 等 | 主服务只新增 `[drone] service_addr`（client 地址）；drone-service 用独立 `drone.toml` |

### 2.2 需要新增的依赖

| 依赖 | 用途 | 位置 |
|---|---|---|
| `tonic 0.12`、`prost 0.13`、`prost-types 0.13`、`tonic-build 0.12` | gRPC 契约与代码生成 | workspace |
| `tokio-stream 0.1` | `ReceiverStream`、gRPC streaming | workspace |
| `geo 0.29`、`geo-types 0.7`、`geojson 0.24` | 围栏几何判断与 GeoJSON 解析 | workspace（仅 rustcam-drone-service 依赖） |
| `h3o 0.7` | 围栏粗筛（可选） | workspace（仅 rustcam-drone-service 依赖） |
| `mavlink 0.21`（`std` + `tokio` + UDP/TCP/serial，`dialect-ardupilotmega`） | adapter 连接 PX4/ArduPilot | rustcam-drone-adapter |
| `cesium`、`vite-plugin-cesium` | 3D 姿态与限高围栏 | frontend |
| `zod`（可选）或手写校验 | 前端运行时消息校验 | frontend |

> 注意：`tonic`/`prost` 会带来一定编译时间增长，可接受；proto 文件保持精简。`protoc` 需可用，或在 `rustcam-drone-proto/build.rs` 使用 `protoc-bin-vendored` 避免环境依赖。

### 2.3 明确不做

- 不把 MAVLink 解析、厂商 SDK 调用放进 `rustcam-media` 主进程。
- 不让前端直连 `rustcam-drone-service` 或 adapter。
- 不在热路径逐条查询 PostGIS。
- 不把 drone 控制面塞进 `rustcam-media` 进程；`rustcam-drone-service` 从第一天就是独立进程/独立 crate。
- 不抽离现有 `EventBus`；drone 使用自己的进程内总线，跨进程走 Redis Streams/gRPC。
- 不建 `rustcam-infra` 大杂烩 crate；共享代码只保留 `rustcam-drone-proto` 与 `rustcam-drone-core`。
- 不让 `rustcam-drone-service` 依赖 `rustcam-media`（避免反向依赖与体积膨胀）；DB/Redis/metrics 各进程各写薄封装。
- 不引入 Java；C++ 仅用于必须的厂商 SDK sidecar。
- 不为了“统一”重写现有 SIP/媒体/录制/权限逻辑。

---

## 3. 总体架构

### 3.1 组件图

```text
┌──────────────────────────────────────────────────────────────┐
│ Desktop / Browser (Qt + CEF 或直接浏览器)                     │
│  ├─ MapLibre + terra-draw     2D 围栏/轨迹                    │
│  ├─ Cesium                    3D 姿态/限高/回放                │
│  └─ REST + WebSocket                                          │
└───────────────┬──────────────────────────────────────────────┘
                │ HTTP / WS（只认 rustcam-server）
┌───────────────▼──────────────────────────────────────────────┐
│ rustcam-server / rustcam-media（现有，保持稳定）               │
│  ├─ SIP / 媒体 / ONVIF / RTSP / 录制（不动）                   │
│  ├─ API Gateway：/api/v1/drone/*                              │
│  ├─ WS Gateway：/ws/drone/*（订阅 drone 事件流后转发）          │
│  └─ 鉴权：JWT + Casbin，operator 注入                         │
└───────┬───────────────────────────────┬──────────────────────┘
        │ gRPC(ControlService/QueryService/GeofenceService + EventService)   │ Redis
        │                               │
┌───────▼──────────────────────────────▼──────────────────────┐
│ rustcam-drone-service（独立进程）                              │
│  ├─ 控制面：分控/指令/遥测/围栏/风控/注册表/审计                │
│  ├─ gRPC server：AdapterLink(adapter) + Control/Query/Geofence │
│  ├─ 自己的 Redis 客户端（Lua/Streams）与 PG 连接池             │
│  └─ 内网 /metrics（运维）                                      │
└───────┬──────────────────────────────────────────────────────┘
        │ gRPC(AdapterLink, bidi)
┌───────▼──────────────┐        ┌───────────────────────────────┐
│ rustcam-drone-adapter │        │ Redis                         │
│  ├─ bidi client       │        │  Streams: telemetry/events/   │
│  ├─ MAVLink 连接池    │        │           cmd_reply/alert     │
│  ├─ 遥测规范化        │        │  Keys: control/registry/      │
│  ├─ 指令执行/回执     │        │        shadow/geofence_ver    │
│  └─ 厂商 sidecar      │        └───────────────────────────────┘
└───────┬──────────────┘
        │ MAVLink / DJI Cloud API / SDK sidecar
┌───────▼──────────────────────────────────────────────────────┐
│ PX4 SITL / ArduPilot / 真机 / DJI / 吊舱                       │
└──────────────────────────────────────────────────────────────┘

存储：PostgreSQL（MVP 围栏/审计，drone_* 表由 rustcam-drone-service 拥有）
      TimescaleDB（轨迹历史，后置）
      可选 MinIO（录像/日志对象存储）
观测：tracing + Prometheus（rustcam-server 与 rustcam-drone-service 各自暴露 /metrics）+ Grafana + Loki
```

### 3.2 职责边界

| 组件 | 做 | 不做 |
|---|---|---|
| `rustcam-server` | SIP/媒体、API 网关、WS 网关、鉴权入口、drone 唯一对外入口（REST/WS → gRPC） | 不做分控仲裁、不做遥测处理、不直接操作 drone 表 |
| `rustcam-drone-service` | 控制面、指令、遥测、围栏、风控、注册表、审计；只暴露内网 gRPC | 不提供对外 HTTP、不做前端鉴权、不解析 MAVLink、不依赖 rustcam-media |
| `rustcam-drone-adapter` | MAVLink/Cloud API/SDK 适配、连接管理、编解码、回执 | 不做分控、不做业务规则、无业务状态 |
| Redis | 跨进程事件通道（Streams/PubSub）、控制权锁/租约、注册表、设备影子、幂等键 | 不做持久化权威存储 |
| PostgreSQL/PostGIS | 围栏权威存储、控制/指令审计、设备元数据 | 不在热路径逐条查 |
| TimescaleDB | 轨迹/遥测历史、回放查询 | 不参与实时控制 |
| 前端 | 展示、编辑、下发操作 | 不做风控判断、不直连 adapter / drone-service |
| PX4/ArduPilot | 飞控、状态估计、执行 | 不承载业务逻辑 |
| C++ sidecar | 仅厂商 SDK（PSDK/吊舱）封装 | 不做分控/业务 |

### 3.3 关键设计决策

| 决策 | 选择 | 理由 | 后续可逆性 |
|---|---|---|---|
| 仓库拆分 | 最小 workspace，原 lib 不细拆；只共享 proto/core | 降低迁移风险，避免 `pub(crate)` 与循环依赖爆炸 | 可逐步拆细 |
| 事件总线 | 不抽现有 `EventBus` | 共享少，避免为共享而共享；跨进程用 Streams/gRPC | 出现第二个真实消费者再抽 |
| drone 业务位置 | **独立进程** `rustcam-drone-service`，从第一天就与 rustcam-media 分开 | 进程边界清晰、可单独扩缩容、故障隔离；避免控制面逻辑侵入 SIP 主服务 | 进程内嵌回退代价高，不建议回退 |
| 服务边界 | gRPC bidi + proto 契约 | adapter 主动出站、易穿 NAT、双向实时 | 可换 MQTT |
| 控制权 | Redis Lua 原子脚本 | 读-判断-写一次完成，支持抢占与多实例 | 可换 Functions/WATCH |
| 围栏 | MVP GeoJSON text + `geo` | rbatis 对 PostGIS 类型支持弱，先绕开 | 可迁移 PostGIS |
| 遥测 | gRPC 主路径，Streams 异步旁路 | 实时性与解耦兼顾 | 可纯 Streams |
| 前端 | 现有 Vue 工程加 `/drone/*` | 不新建项目，鉴权/构建/部署复用 | — |
| 语言 | JS + JSDoc + 运行时校验 | 与现有前端一致，避免引入 TS 构建 | 可渐进 TS |

---

## 4. Workspace 拆分

### 4.1 目标目录

```text
RustCam-Media/                        # 仓库根 = workspace 根
├── Cargo.toml                        # [workspace]
├── Cargo.lock                        # 只保留根一份
├── config.toml                       # 主服务配置（不动 drone 段）
├── drone.toml                        # 新增：rustcam-drone-service 自己的配置
├── frontend/                         # Vue 3，独立构建，不进 workspace
│   ├── package.json                  # 新增 cesium / vite-plugin-cesium
│   └── src/
│       ├── router/index.js           # 新增 /drone/*（追加到 AppLayout 的 children）
│       ├── stores/drone*.js          # 新增（stores 为平铺目录）
│       ├── api/drone.js              # 新增
│       ├── api/droneWs.js            # 新增
│       ├── composables/useDroneWs.js # 新增（composables 为平铺目录）
│       ├── utils/droneValidate.js    # 新增（utils 为平铺目录）
│       ├── views/drone/              # 新增页面
│       └── components/drone/         # 新增组件
├── sql/
│   └── schema.sql                    # 主服务表结构（drone_* 表由 drone-service 自带迁移）
├── conf/, docs/, onvif/, rtsp-player/, tests/   # 保持
├── crates/
│   ├── rustcam-media/                # 原 lib 整体迁入
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── api/ application/ auth/ common/ context/
│   │       ├── domain/ infrastructure/ media/ monitoring/
│   │       ├── protocol/ transport/
│   │       └── drone_gateway/         # 新增：仅网关层（gRPC client + REST/WS handler）
│   │           ├── mod.rs
│   │           ├── client.rs          # DroneClient（tonic client 封装）
│   │           ├── api.rs             # /api/v1/drone/* handler
│   │           ├── ws.rs              # /ws/drone/* handler（订阅 EventService）
│   │           └── ticket.rs          # ws-ticket 签发与校验
│   ├── rustcam-server/               # 原 main.rs
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   ├── rustcam-drone-proto/          # 新增：契约
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── proto/{common,adapter_link,control}.proto
│   │   └── src/lib.rs
│   ├── rustcam-drone-core/           # 新增：纯类型/常量，service + adapter 共享
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── telemetry.rs
│   │       ├── command.rs
│   │       ├── event.rs
│   │       ├── keys.rs
│   │       └── metrics.rs            # drone 指标文本输出（两个进程内共用）
│   ├── rustcam-drone-service/        # 新增：控制面独立进程
│   │   ├── Cargo.toml
│   │   ├── migrations.sql            # drone_* 表幂等 DDL（启动时执行）
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── state.rs
│   │       ├── infra/{redis.rs,db.rs,metrics.rs}
│   │       ├── registry.rs
│   │       ├── redis_keys.rs
│   │       ├── grpc/{mod.rs,adapter_link.rs,control.rs,query.rs,geofence.rs,event.rs}
│   │       ├── control/{mod.rs,lease.rs,priority.rs,audit.rs}
│   │       ├── command/{mod.rs,idempotency.rs,timeout.rs,router.rs}
│   │       ├── telemetry/{mod.rs,dedup.rs,shadow.rs,broadcast.rs}
│   │       ├── geofence/{mod.rs,store.rs,eval.rs,cache.rs}
│   │       ├── risk/{mod.rs,rules.rs}
│   │       └── lua/{acquire,release,renew,preempt}.lua
│   └── rustcam-drone-adapter/        # 新增：协议适配独立进程
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── link/{mod,reconnect,session}.rs
│           ├── mavlink/{mod,conn,telemetry,command,heartbeat}.rs
│           ├── dji/{mod,mapping}.rs
│           ├── sidecar/grpc_sidecar.rs
│           ├── buffer/ring.rs
│           └── observability.rs
└── target/, node_modules/, .idea/     # .gitignore
```

`rustcam-drone-service` 是独立 crate + 独立进程，从第一天就存在；`rustcam-media` 里没有 `src/drone/`，主服务只保留对外网关（REST + WS），通过 gRPC 调 drone-service。

共享边界：只共享 `rustcam-drone-proto` 与 `rustcam-drone-core`；现有 `EventBus` 不抽，`rustcam-infra` 不建。`rustcam-drone-service` 使用自己的进程内总线，跨进程事件走 Redis Streams/gRPC。

### 4.2 根 Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/rustcam-media",
    "crates/rustcam-server",
    "crates/rustcam-drone-proto",
    "crates/rustcam-drone-core",
    "crates/rustcam-drone-service",
    "crates/rustcam-drone-adapter",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["RustCam Team"]

[workspace.dependencies]
tokio = { version = "1.54.0", features = ["full"] }
axum = { version = "0.8.9", features = ["ws", "macros", "tokio", "http1"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"
reqwest = { version = "0.13.3", features = ["json", "query"] }
redis = { version = "1.7.1", features = ["tokio-comp", "connection-manager"] }
rbatis = { version = "4.9.8", features = ["debug_mode"] }
rbdc-pg = "4.9.6"
rbdc = "4.9.10"
bb8 = "0.9.1"
rbs = "4"
url = "2"
anyhow = "1.0.104"
thiserror = "2.0.21"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
tracing-log = "0.2"
uuid = { version = "1.28.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
config = "0.15.27"
futures = "0.3.34"
futures-util = "0.3"
futures-core = "0.3"
tower = "0.5.3"
tower-http = { version = "0.7.1", features = ["cors", "trace", "timeout"] }
hyper = "1.12.0"
hyper-util = { version = "0.1.21", features = ["tokio", "server", "http1"] }
rustls = { version = "0.23.46", features = ["ring"] }
rustls-pemfile = "2.1"
tokio-rustls = "0.26.6"
parking_lot = "0.13.0"
dashmap = "6.2.1"
once_cell = "1.19"
toml = "1.1.8+spec-1.1.0"
rand = { version = "0.10", features = ["std", "std_rng", "thread_rng"] }
rand_core = "0.10"
getrandom = "0.4.3"
bytes = "1.12.1"
jsonwebtoken = { version = "11.0.0", features = ["aws_lc_rs"] }
casbin = "2"
argon2 = "0.6.0"
password-hash = { version = "0.6", features = ["rand_core", "phc"] }
async-trait = "0.1.92"
regex = "1.13.1"
tokio-util = "0.7.20"
rubato = "5.0.1"
rsip = "0.4"
quick-xml = "0.42.0"
roxmltree = "0.21.1"
memchr = "2.8.3"
sha1 = "0.11.0"
cron = "0.17.0"
base64 = "0.23.1"
md5 = "0.8.1"

# 新增：drone
tonic = "0.12"
prost = "0.13"
prost-types = "0.13"
tokio-stream = "0.1"
geo = "0.29"
geo-types = "0.7"
geojson = "0.24"
h3o = "0.7"
mavlink = { version = "0.13", features = ["std"] }

[profile.dev]
opt-level = 0
debug = true

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

### 4.3 crates/rustcam-media/Cargo.toml

```toml
[package]
name = "rustcam-media"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Streaming media signaling management platform"

[lib]
name = "rustcam_media"
path = "src/lib.rs"

[dependencies]
# 原依赖全部改为 workspace = true
tokio = { workspace = true }
axum = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
redis = { workspace = true }
rbatis = { workspace = true }
rbdc-pg = { workspace = true }
# ... 其余原依赖

# drone（仅网关侧：gRPC client + 类型）
tonic = { workspace = true }
prost = { workspace = true }
prost-types = { workspace = true }
tokio-stream = { workspace = true }
rustcam-drone-proto = { path = "../rustcam-drone-proto" }
rustcam-drone-core = { path = "../rustcam-drone-core" }

[dev-dependencies]
tokio-test = "0.4.6"
mockall = "0.15.0"
```

关键：**去掉 `[[bin]]`，只保留 `[lib]`**。主服务 lib 只做 drone 的对外网关（REST + WS），不包含控制面逻辑，因此不依赖 `geo`/`rbatis` 侧的围栏实现——围栏在 drone-service 侧。

### 4.4 crates/rustcam-server/Cargo.toml

```toml
[package]
name = "rustcam-server"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "RustCam binary entrypoint"

[[bin]]
name = "rustcam-media"          # bin 名保持原样，部署脚本不用改
path = "src/main.rs"

[dependencies]
rustcam-media = { path = "../rustcam-media" }
tokio = { workspace = true }
axum = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tracing-appender = { workspace = true }
tracing-log = { workspace = true }
anyhow = { workspace = true }
jsonwebtoken = { workspace = true }
futures = { workspace = true }
```

### 4.5 crates/rustcam-drone-proto/Cargo.toml

```toml
[package]
name = "rustcam-drone-proto"
version.workspace = true
edition.workspace = true

[dependencies]
tonic = { workspace = true }
prost = { workspace = true }
prost-types = { workspace = true }

[build-dependencies]
tonic-build = "0.12"
```

### 4.6 crates/rustcam-drone-core/Cargo.toml

```toml
[package]
name = "rustcam-drone-core"
version.workspace = true
edition.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
```

`rustcam-drone-core` 只放跨进程稳定类型：`Telemetry`、`Command`、`DroneEvent`、Redis key/stream 常量。不依赖 tokio、axum、rbatis、redis，也不放 EventBus 实现。

### 4.7 crates/rustcam-drone-service/Cargo.toml

```toml
[package]
name = "rustcam-drone-service"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "RustCam drone control plane service"

[[bin]]
name = "rustcam-drone-service"
path = "src/main.rs"

[dependencies]
rustcam-drone-proto = { path = "../rustcam-drone-proto" }
rustcam-drone-core = { path = "../rustcam-drone-core" }
tokio = { workspace = true }
tokio-stream = { workspace = true }
tonic = { workspace = true }
prost = { workspace = true }
prost-types = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
redis = { workspace = true }
rbatis = { workspace = true }
rbdc-pg = { workspace = true }
geo = { workspace = true }
geo-types = { workspace = true }
geojson = { workspace = true }
h3o = { workspace = true }
dashmap = { workspace = true }
config = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
tracing-appender = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
```

**不依赖 `rustcam-media`**：drone-service 自带 Redis 客户端、PG 连接池、指标输出与配置加载。`rustcam-drone-core` 里只放纯类型与 Redis key/stream 常量（不含实现）。

### 4.8 crates/rustcam-drone-adapter/Cargo.toml

```toml
[package]
name = "rustcam-drone-adapter"
version.workspace = true
edition.workspace = true

[[bin]]
name = "rustcam-drone-adapter"
path = "src/main.rs"

[dependencies]
rustcam-drone-proto = { path = "../rustcam-drone-proto" }
rustcam-drone-core = { path = "../rustcam-drone-core" }
tokio = { workspace = true }
tokio-stream = { workspace = true }
tonic = { workspace = true }
prost = { workspace = true }
mavlink = { workspace = true }
redis = { workspace = true }          # 可选：遥测旁路到 Streams 时需要
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

### 4.9 共享边界与 EventBus 决策

- 共享：`rustcam-drone-proto`（gRPC 契约）、`rustcam-drone-core`（Telemetry/Command/Event/Redis key/stream 常量/metrics 文本输出）。
- 不共享：现有 `EventBus`、`ServiceRegistry`、`DbRepository`、`RedisStore`、axum router、`WsBroadcaster`、media/SIP/ONVIF/RTSP 代码。
- 不建 `rustcam-infra`；drone-service 的 Redis/PG/metrics 均为自写薄封装（与主服务少量重复是刻意接受的代价）。
- `rustcam-media` 保留 `EventBus<SignalEvent>`；`rustcam-drone-service` 使用自己的 `tokio::broadcast` 或轻量本地总线。
- 跨进程事件走 Redis Streams / PubSub / gRPC，不使用进程内 EventBus。
- 主服务 → drone-service 的调用一律走 gRPC（`ControlService`/`QueryService`/`GeofenceService`/`EventService`），不直连 Redis/PG 的 drone_* 表。

### 4.10 迁移顺序（每步可编译、可回滚）

| 步 | 操作 | 验证 |
|---|---|---|
| 1 | 根 `Cargo.toml` 改 `[workspace]`，保留原 `[package]` 内容暂不动 | `cargo metadata` 通过 |
| 2 | 新建 `crates/rustcam-media/Cargo.toml`，迁入原 `[lib]` 与依赖 | — |
| 3 | 原 `src/` 除 `main.rs` 全部移入 `crates/rustcam-media/src/` | `cargo check -p rustcam-media` |
| 4 | 新建 `crates/rustcam-server/`，迁入 `main.rs` | `cargo check -p rustcam-server` |
| 5 | 根 `Cargo.toml` 删除 `[package]/[lib]/[[bin]]`，members 指向新 crate | `cargo build --bin rustcam-media` |
| 6 | 新建 `rustcam-drone-proto` 与 `rustcam-drone-core`（proto + 纯类型） | `cargo check -p rustcam-drone-proto -p rustcam-drone-core` |
| 7 | 新建 `crates/rustcam-drone-service/`（bin + config + infra + state） | `cargo check -p rustcam-drone-service` |
| 8 | drone-service：gRPC server（AdapterLink + Control/Query/Geofence/Event）+ `migrations.sql` | 服务起表建好，gRPC :50051 监听 |
| 9 | drone-service：注册表 + 遥测去重/影子 + EventService 广播 | 假 adapter 注册成功，事件流可订阅 |
| 10 | `rustcam-media`：drone gRPC client + `GET /api/v1/drone/ping`（挂进 `protected_routes`） | 现有服务正常；无 token 返回 401 |
| 11 | 主服务：WS 网关 `/ws/drone/*`（订阅 EventService）+ `ws-ticket` 鉴权 | 浏览器收到遥测；无 ticket 连接被拒 |
| 12 | 新建 `rustcam-drone-adapter` 假实现（定时假遥测） | adapter 注册成功，WS 收到 |
| 13 | drone-service：控制权 Lua + acquire/release/renew/preempt/force_release | 两个客户端抢权互斥 |
| 14 | 主服务：drone REST 全量 + Casbin enforce（策略来自数据库 `permissions`/`role_permissions`） | 越权返回 403 |
| 15 | drone-service：指令状态机 + 下行 + 回执 | 指令状态闭环 |
| 16 | drone-service：围栏 CRUD + 越界告警 | 围栏可读写、越界触发 high 告警 |
| 17 | adapter 换 `rust-mavlink` 连 PX4 SITL | 真实遥测 |
| 18 | 前端 Vue 页面 + terra-draw + Cesium | 端到端 |

---

## 5. 契约层：rustcam-drone-proto

### 5.1 包与模块

| proto | package | Rust 模块 | 用途 |
|---|---|---|---|
| `common.proto` | `drone.v1.common` | `rustcam_drone_proto::common` | 遥测、指令、回执、设备事件 |
| `adapter_link.proto` | `drone.v1.link` | `rustcam_drone_proto::link` | adapter ↔ service bidi |
| `control.proto` | `drone.v1.control` | `rustcam_drone_proto::control` | 控制权、查询、围栏、事件流 |

> proto 的 package 命名空间保持 `drone.v1.*`（线上契约命名空间，不随 crate 改名而变）；Rust crate 名带 `rustcam-` 前缀。

### 5.2 build.rs

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(
            &[
                "proto/common.proto",
                "proto/adapter_link.proto",
                "proto/control.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
```

如 CI 无 `protoc`，可加 `protoc-bin-vendored` 并在 build.rs 设置 `PROTOC` 环境变量。

### 5.3 src/lib.rs

```rust
pub mod common {
    tonic::include_proto!("drone.v1.common");
}

pub mod link {
    tonic::include_proto!("drone.v1.link");
}

pub mod control {
    tonic::include_proto!("drone.v1.control");
}
```

### 5.4 proto/common.proto

```proto
syntax = "proto3";
package drone.v1.common;

message Empty {}

message Telemetry {
  string drone_tag = 1;
  int64  ts_ms = 2;

  double lat = 3;          // 度
  double lon = 4;          // 度
  double alt_amsl = 5;     // 米，海拔
  double rel_alt = 6;      // 米，相对起飞点

  double roll = 7;         // 弧度
  double pitch = 8;        // 弧度
  double yaw = 9;          // 弧度

  double qx = 10;
  double qy = 11;
  double qz = 12;
  double qw = 13;

  double vx = 14;          // m/s，NED
  double vy = 15;
  double vz = 16;

  double battery_pct = 17;
  double voltage = 18;
  uint32 link_quality = 19; // 0-100
  uint32 seq = 20;          // 每设备单调递增
}

message Command {
  string cmd_id = 1;                 // 幂等键，service 生成或客户端提供
  string drone_tag = 2;
  string type = 3;                   // rtl / takeoff / land / goto / gimbal / camera
  map<string, string> params = 4;
  int64  deadline_ms = 5;            // 绝对时间，超时即失败
  string operator = 6;               // 由 rustcam-server 从 JWT 注入，不信任客户端
}

message CommandReply {
  string cmd_id = 1;
  string drone_tag = 2;
  string status = 3;                 // acked / executed / failed / timeout / rejected
  int32  code = 4;
  string message = 5;
  int64  ts_ms = 6;
}

message DeviceEvent {
  string drone_tag = 1;
  string type = 2;                   // connected / disconnected / link_lost / error
  string detail = 3;
  int64  ts_ms = 4;
}
```

### 5.5 proto/adapter_link.proto

```proto
syntax = "proto3";
package drone.v1.link;

import "common.proto";

service AdapterLink {
  // adapter 启动即连；首帧必须是 Register
  // RPC 不能命名为 Connect：tonic 会生成同名 connect(dst) 构造器，导致 E0592 重复定义
  rpc Open (stream AdapterMessage) returns (stream ServiceMessage);
}

message AdapterMessage {
  oneof payload {
    Register            register      = 1;
    Heartbeat           heartbeat     = 2;
    common.Telemetry    telemetry     = 3;
    common.CommandReply command_reply = 4;
    common.DeviceEvent  event         = 5;
  }
}

message ServiceMessage {
  oneof payload {
    RegisterAck  register_ack  = 1;
    common.Command command     = 2;
    ControlState control_state = 3;
  }
}

message Register {
  string adapter_id = 1;
  string protocol   = 2;            // mavlink / dji-cloud / gimbal
  string version    = 3;
}

message RegisterAck {
  bool accepted = 1;
  string reason = 2;
  repeated string assigned_drone_tags = 3;
  int32 heartbeat_interval_sec = 4;
}

message Heartbeat {
  int64 ts_ms = 1;
  int32 device_count = 2;
  int32 inflight_commands = 3;
  string adapter_state = 4;         // ok / degraded
}

message ControlState {
  string drone_tag = 1;
  string control_type = 2;
  string holder = 3;
  int32  priority = 4;
  int64  expire_at_ms = 5;
  string action = 6;                 // acquired / renewed / released / expired / preempted
}
```

### 5.6 proto/control.proto

```proto
syntax = "proto3";
package drone.v1.control;

import "common.proto";

service ControlService {
  rpc AcquireControl (AcquireRequest) returns (Lease);
  rpc ReleaseControl (ReleaseRequest) returns (ReleaseReply);
  rpc PreemptControl (PreemptRequest) returns (Lease);
  rpc RenewControl   (RenewRequest)   returns (Lease);
  rpc ForceReleaseControl (ForceReleaseRequest) returns (ReleaseReply);
  rpc GetControlState(StateRequest)   returns (StateReply);
  rpc SendCommand    (common.Command) returns (common.CommandReply);
}

service QueryService {
  rpc GetDevice     (DeviceRequest) returns (DeviceInfo);
  rpc ListDevices   (common.Empty)  returns (DeviceList);
  rpc GetTelemetry  (DeviceRequest) returns (common.Telemetry);
  rpc GetCommandStatus (CommandStatusRequest) returns (CommandStatus);
  rpc ListAlerts       (AlertListRequest)     returns (AlertList);
}

// 围栏 CRUD（proto 契约先行，联调前补齐）
service GeofenceService {
  rpc ListGeofences  (GeofenceListRequest)   returns (GeofenceList);
  rpc CreateGeofence (GeofenceUpsertRequest) returns (Geofence);
  rpc UpdateGeofence (GeofenceUpsertRequest) returns (Geofence);
  rpc DeleteGeofence (GeofenceDeleteRequest) returns (DeleteReply);
}

// drone-service → rustcam-server：事件流（遥测/告警/控制权/指令回执）
// rustcam-server 订阅后按 msg_type 过滤，推给浏览器 WS
service EventService {
  rpc Subscribe (SubscribeRequest) returns (stream EventMessage);
}

message SubscribeRequest {
  repeated string drone_tags = 1;   // 空 = 订阅全部设备
}

message EventMessage {
  string msg_type = 1;              // telemetry / alert / control / command
  string payload_json = 2;          // data（JSON 字符串，避免再定义一套嵌套结构）
  int64  ts_ms = 3;
}

message AcquireRequest {
  string drone_tag = 1;
  string control_type = 2;          // flight / gimbal / mission
  string operator = 3;              // 由 rustcam-server 从 JWT 注入
  int32  priority = 4;
  int32  lease_seconds = 5;
}

message ReleaseRequest {
  string drone_tag = 1;
  string control_type = 2;
  string operator = 3;
}

message ReleaseReply {
  bool released = 1;
  string reason = 2;
}

// 管理员强制释放（不受 holder 限制）
message ForceReleaseRequest {
  string drone_tag = 1;
  string control_type = 2;
  string operator = 3;            // 由 rustcam-server 从 JWT 注入
}

message PreemptRequest {
  string drone_tag = 1;
  string control_type = 2;
  string operator = 3;
  int32  priority = 4;
  int32  lease_seconds = 5;
}

message RenewRequest {
  string drone_tag = 1;
  string control_type = 2;
  string operator = 3;
  int32  lease_seconds = 4;
}

message Lease {
  string drone_tag = 1;
  string control_type = 2;
  string holder = 3;
  int32  priority = 4;
  int64  expire_at_ms = 5;
  string preempted = 6;             // 被抢占者，空表示无
}

message StateRequest {
  string drone_tag = 1;
  string control_type = 2;
}

message StateReply {
  bool held = 1;
  Lease lease = 2;
}

message DeviceRequest {
  string drone_tag = 1;
}

message DeviceInfo {
  string drone_tag = 1;
  string protocol = 2;
  string adapter_id = 3;
  bool   online = 4;
  int64  last_seen_ms = 5;
  common.Telemetry last_telemetry = 6;
}

message DeviceList {
  repeated DeviceInfo devices = 1;
}

// 以下消息与上面的 RPC 一一对应，联调前补齐定义
message CommandStatusRequest { string cmd_id = 1; }
message CommandStatus { string cmd_id = 1; string status = 2; string message = 3; }
message AlertListRequest { string drone_tag = 1; int32 limit = 2; }
message Alert { string drone_tag = 1; string rule = 2; string level = 3; string message = 4; int64 ts_ms = 5; }
message AlertList { repeated Alert alerts = 1; }
message GeofenceListRequest { bool only_enabled = 1; }
message GeofenceList { repeated Geofence geofences = 1; }
message Geofence {
  string id = 1;
  string name = 2;
  string type = 3;              // no_fly / work / height
  string geojson = 4;
  double min_alt = 5;
  double max_alt = 6;
  bool enabled = 7;
  int32 version = 8;
}
message GeofenceUpsertRequest { Geofence geofence = 1; string operator = 2; }
message GeofenceDeleteRequest { string id = 1; string operator = 2; }
message DeleteReply { bool deleted = 1; string reason = 2; }
```

### 5.7 契约兼容规则

- proto 字段只增不改，删除字段用 `reserved`。
- `oneof` 新增分支保持向后兼容；adapter 与 service 版本不一致时忽略未知分支。
- `RegisterAck` 返回 `heartbeat_interval_sec`，adapter 不得写死。
- 所有时间字段统一 `int64 ts_ms`（Unix 毫秒，UTC）。
- 角度统一弧度，高度统一米，速度统一 m/s。

### 5.8 契约增补：设备唯一标识与服务端分配

语义与威胁模型见 §12.4。字段只增不改，编号取各消息的下一个可用号。

```proto
// proto/common.proto
message Telemetry {
  // ... 既有字段 1-29 ...
  string unique_id = 30;   // 该帧所属飞控的唯一标识；空 = 飞控未提供
}

// proto/adapter_link.proto
message DeviceIdentity {
  string drone_tag = 1;    // 服务端分配的无人机标识
  string unique_id = 2;    // 该设备对应飞控的唯一标识；空 = 未读到
}

message IdentityReport {
  repeated DeviceIdentity identities = 1;  // adapter 建链后上报的实测标识
}

message Register {
  string adapter_id = 1;
  string protocol   = 2;            // mavlink / dji-cloud / gimbal
  string version    = 3;
}

message DeviceAssignment {
  string drone_tag = 1;      // 服务端分配给本 adapter 的无人机标识
  string mavlink_addr = 2;   // 该机的 MAVLink 端点（隧道内地址）
  string signing_secret = 3; // 该机的 MAVLink 2 签名密钥（32 字节 hex）；空 = 不启用签名
  bool allow_unsigned = 4;   // 是否放行未签名帧（含 MAVLink 1 帧）；仅在有密钥时生效
  int32 mavlink_version = 5; // 期望的 MAVLink 协议版本（1/2）
  int32 mavlink_sysid = 6;   // 期望的飞控 system id（1..255）
  int32 mavlink_compid = 7;  // 期望的飞控 component id
}

message RegisterAck {
  bool accepted = 1;
  string reason = 2;
  repeated string assigned_drone_tags = 3;
  int32 heartbeat_interval_sec = 4;
  repeated DeviceAssignment assignments = 5;  // 每台机的端点、密钥与 sysid/compid；与 assigned_drone_tags 同序
}

// proto/control.proto
message DroneProfile {
  string drone_tag = 1;       // 平台唯一标识（原设备 ID 与 drone_tag 合并）
  // ... 既有字段 2-12 ...
  string unique_id = 13;      // 平台登记的无人机唯一标识；空 = 未登记
  string mavlink_addr = 14;   // 该机的 MAVLink 端点（隧道内地址）
  string signing_secret = 15; // 该机的 MAVLink 2 签名密钥；空 = 不启用签名
  bool signing_allow_unsigned = 16;  // 是否放行未签名帧（含 MAVLink 1 帧）；默认 true
  int32 mavlink_version = 17; // 期望的 MAVLink 协议版本（1/2）
  int32 mavlink_sysid = 18;   // 期望的飞控 system id（1..255）
  int32 mavlink_compid = 19;  // 期望的飞控 component id
}
```

约定：

- `drone_tag` 是唯一标识：`drone-<uuid4>`，由服务端建档时生成、不可变；REST 路径/查询、前端 key、Redis 键、审计表全部使用它。
- `unique_id` 统一用小写十六进制字符串（不带 `0x` 前缀）。`AUTOPILOT_VERSION.uid` 是 64 位，多字节序随固件实现不同，因此**以 adapter 上报的实测值为准**登记，不在服务端做字节序推断。
- `Register` 不再携带设备清单或唯一标识：adapter 上线只报 `adapter_id` + `protocol` + `version`，设备由服务端按 `adapter_id` 分配。
- `IdentityReport.identities` 由 adapter **建链后**上报；某条目的 `unique_id` 为空 → 视为"未提供唯一标识"，判定规则见 §12.4（档案已登记唯一标识时按不一致处理）。不一致即撤销该机分配。
- 注册阶段不校验唯一标识（此时尚未连飞控、拿不到 uid）；校验统一收敛到 `IdentityReport` 与指令下发前的 `check_identity`。
- `Telemetry.unique_id` 每帧携带，供 service 做换机检测（档案登记值与实测值不一致时拒绝下发指令并告警）。
- `uid2` 不进契约：其字节序与含义随固件变化，不能作为稳定标识。
- 端点、密钥、期望 sysid/compid 的唯一真相源是 `drone_device` 表（§9.1），adapter.toml 不再承载这些项。
- `RegisterAck.assignments` 随分配下发；`assigned_drone_tags` 保留（兼容旧 adapter），两者同序一一对应。
- `signing_secret` 与 `mavlink_addr` 属敏感分配数据，只在已认证的 gRPC 通道上下发（§12.2 mTLS 为前置）。
- `signing_secret` 为空表示该机**不需要签名**（§12.4 L2）；非空时飞控侧必须配同一把密钥。
- `allow_unsigned` 决定未签名帧（含 MAVLink 1 帧）放不放行，仅在有密钥时生效；混发 v1/v2 的
  链路必须为 `true`，否则会丢掉全部 MAVLink 1 帧（§12.4 L2）。

---

## 6. rustcam-drone-service 详细设计（独立进程）

### 6.1 模块目录

```text
crates/rustcam-drone-service/
├── migrations.sql             # drone_* 表幂等 DDL（启动时执行）
└── src/
    ├── main.rs                # 进程入口：配置 → infra → state → gRPC server
    ├── config.rs              # DroneConfig（自有 drone.toml / 环境变量）
    ├── state.rs               # DroneState
    ├── infra/
    │   ├── redis.rs           # DroneRedis：Lua / Streams / SET NX
    │   ├── db.rs              # DroneDb：连接池 + migrations.sql 执行
    │   └── metrics.rs         # 指标收集（输出到 rustcam-drone-core 的文本格式）
    ├── registry.rs            # adapter/设备注册表 + reaper
    ├── redis_keys.rs          # 所有 Redis key / Stream 名常量（唯一来源）
    ├── grpc/
    │   ├── mod.rs             # gRPC server 组装
    │   ├── adapter_link.rs    # bidi server（adapter 接入）
    │   ├── control.rs         # ControlService
    │   ├── query.rs           # QueryService
    │   ├── geofence.rs        # GeofenceService
    │   └── event.rs           # EventService（事件流，推给 rustcam-server）
    ├── control/
    │   ├── mod.rs             # 仲裁入口
    │   ├── lease.rs           # EVALSHA 调用
    │   ├── priority.rs        # 优先级规则
    │   └── audit.rs           # 审计写库
    ├── command/
    │   ├── mod.rs             # 状态机
    │   ├── idempotency.rs     # cmd_id 去重
    │   ├── timeout.rs         # deadline 扫描
    │   └── router.rs          # drone_tag → adapter
    ├── telemetry/
    │   ├── mod.rs             # 入站处理
    │   ├── dedup.rs           # drone_tag + seq
    │   ├── shadow.rs          # Redis 设备影子
    │   └── broadcast.rs       # broadcast → EventService
    ├── geofence/
    │   ├── mod.rs
    │   ├── store.rs           # 加载/保存
    │   ├── eval.rs            # geo 判断
    │   └── cache.rs           # 内存副本 + 版本
    ├── risk/
    │   ├── mod.rs
    │   └── rules.rs           # 低电量/失联/超速/越界
    └── lua/
        ├── acquire.lua
        ├── release.lua
        ├── renew.lua
        └── preempt.lua
```

> 这里**没有** `api.rs` / `ws.rs`：drone-service 不提供对外 HTTP/WS，HTTP 与浏览器 WS 由 `rustcam-server` 承担，drone-service 只暴露 gRPC。

### 6.2 启动入口

```rust
// crates/rustcam-drone-service/src/main.rs
mod config;
mod state;
mod infra;
mod registry;
mod redis_keys;
mod grpc;
mod control;
mod command;
mod telemetry;
mod geofence;
mod risk;

use rustcam_drone_service::config::DroneConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 配置：drone.toml（可用环境变量覆盖）
    let cfg = DroneConfig::load()?;

    // 2. infra：自己的 Redis 客户端与 PG 连接池（不复用 rustcam-media）
    let redis = infra::redis::DroneRedis::connect(&cfg.redis_url).await?;
    let db = infra::db::DroneDb::connect(&cfg.database_url).await?;
    db.run_migrations().await?;              // 执行 migrations.sql（幂等）

    // 3. 状态
    let state = state::DroneState::new(cfg, redis, db);

    // 4. gRPC server（AdapterLink + Control/Query/Geofence/Event）
    let addr = state.config.grpc_addr.clone();
    let s = state.clone();
    tokio::spawn(async move {
        if let Err(e) = grpc::serve(s, addr.as_str()).await {
            tracing::error!(error = %e, "drone gRPC server stopped");
        }
    });

    // 5. 后台任务
    let s = state.clone();
    tokio::spawn(async move {
        if let Err(e) = telemetry::run_consumer(s).await {
            tracing::error!(error = %e, "drone telemetry consumer stopped");
        }
    });

    let s = state.clone();
    tokio::spawn(async move {
        if let Err(e) = registry::run_adapter_reaper(s).await {
            tracing::error!(error = %e, "drone adapter reaper stopped");
        }
    });

    let s = state.clone();
    tokio::spawn(async move {
        if let Err(e) = command::timeout::run_reaper(s).await {
            tracing::error!(error = %e, "drone command reaper stopped");
        }
    });

    // 6. 内网 /metrics（运维用，与业务 HTTP 无关）
    let metrics_addr = state.config.metrics_addr.clone();
    tokio::spawn(async move {
        if let Err(e) = infra::metrics::serve(metrics_addr.as_str()).await {
            tracing::error!(error = %e, "drone metrics server stopped");
        }
    });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

所有后台任务都拿 `state.clone()`，不要在 `async move` 之后再借用 `state`（避免部分移动导致编译失败）。

### 6.3 配置

drone-service 是独立进程，**不读 rustcam-media 的 `config.toml`**，用自己的 `drone.toml`（可用环境变量覆盖）。

```rust
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DroneConfig {
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "default_metrics_addr")]
    pub metrics_addr: String,
    #[serde(default = "default_heartbeat")]
    pub adapter_heartbeat_sec: u64,
    #[serde(default = "default_lease")]
    pub default_lease_sec: u32,
    #[serde(default)]
    pub risk_enabled: bool,
    #[serde(default = "default_max_inflight")]
    pub max_inflight_commands: usize,
    #[serde(default = "default_command_timeout_sec")]
    pub default_command_timeout_sec: u32,
    // infra 连接
    pub redis_url: String,
    pub database_url: String,
}

fn default_grpc_addr() -> String { "0.0.0.0:50051".into() }
fn default_metrics_addr() -> String { "0.0.0.0:50052".into() }
fn default_heartbeat() -> u64 { 10 }
fn default_lease() -> u32 { 30 }
fn default_max_inflight() -> usize { 1024 }

impl DroneConfig {
    /// 读 drone.toml（不存在则用默认值），环境变量可覆盖
    pub fn load() -> anyhow::Result<Self> { /* config crate + env 覆盖 */ }
}

impl Default for DroneConfig {
    fn default() -> Self {
        Self {
            grpc_addr: default_grpc_addr(),
            metrics_addr: default_metrics_addr(),
            adapter_heartbeat_sec: default_heartbeat(),
            default_lease_sec: default_lease(),
            risk_enabled: false,
            max_inflight_commands: default_max_inflight(),
            redis_url: "redis://127.0.0.1:6379".into(),
            database_url: "postgres://postgres:123456@127.0.0.1:5435/rustcam".into(),
        }
    }
}
```

`drone.toml`：

```toml
grpc_addr = "0.0.0.0:50051"
metrics_addr = "0.0.0.0:50052"
adapter_heartbeat_sec = 10
default_lease_sec = 30
risk_enabled = false
max_inflight_commands = 1024
default_command_timeout_sec = 30
redis_url = "redis://redis:6379"
database_url = "postgres://postgres:123456@postgres:5432/rustcam"
```

环境变量覆盖（容器部署常用）：`DRONE_GRPC_ADDR`、`DRONE_METRICS_ADDR`、`DRONE_REDIS_URL`、`DRONE_DATABASE_URL`、`DRONE_RISK_ENABLED`。

> 主服务的 `config.toml` 仍需一个很小的 `[drone]` 段，但**只放 client 地址**（不再是控制面参数）：`[drone] service_addr = "http://127.0.0.1:50051"`；主服务据此创建 `DroneClient`。

### 6.4 状态

```rust
pub type AdapterTx = tokio::sync::mpsc::Sender<
    rustcam_drone_proto::link::service_message::Payload,
>;

#[derive(Clone)]
pub struct DroneState {
    pub config: std::sync::Arc<DroneConfig>,
    // 自有 infra：不依赖 rustcam-media
    pub redis: std::sync::Arc<crate::infra::redis::DroneRedis>,
    pub db: std::sync::Arc<crate::infra::db::DroneDb>,
    pub adapters: std::sync::Arc<dashmap::DashMap<String, AdapterTx>>,
    pub adapter_generation: std::sync::Arc<dashmap::DashMap<String, u64>>,
    pub device_to_adapter: std::sync::Arc<dashmap::DashMap<String, String>>,
    pub geofence: std::sync::Arc<geofence::cache::GeofenceCache>,
    pub scripts: std::sync::Arc<control::lease::LuaScripts>,
    pub telemetry_tx: tokio::sync::broadcast::Sender<telemetry::TelemetryEvent>,
    pub command_tx: tokio::sync::mpsc::Sender<command::CommandTask>,
    // 事件出口：推给 rustcam-server 的 EventService 订阅者
    pub event_tx: tokio::sync::broadcast::Sender<crate::event::DroneEvent>,
    // 本进程签发的租约表，用于在 key 因 TTL 消失前发现过期并广播 expired
    pub leases: std::sync::Arc<dashmap::DashMap<String, control::lease::LeaseEntry>>,
}
```

`DroneState` 自建 infra，不从 `ServiceRegistry` 取：

```rust
impl DroneState {
    pub async fn new(
        config: DroneConfig,
        redis: Arc<crate::infra::redis::DroneRedis>,
        db: Arc<crate::infra::db::DroneDb>,
    ) -> anyhow::Result<Self> {
        // Redis 不可用时不阻断启动：脚本懒加载 + 降级（见下方说明）
        let scripts = match control::lease::LuaScripts::load(redis.as_ref()).await {
            Ok(s) => std::sync::Arc::new(s),
            Err(e) => {
                tracing::warn!(error = %e, "[Drone] Lua scripts not loaded yet, will retry lazily");
                control::lease::LuaScripts::lazy()
            }
        };

        let geofence = std::sync::Arc::new(
            geofence::cache::GeofenceCache::new(db.clone()).await?
        );
        let (telemetry_tx, _) = tokio::sync::broadcast::channel(4096);
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(1024);
        let (event_tx, _) = tokio::sync::broadcast::channel(4096);
        tokio::spawn(command::run_worker(command_rx));
        Ok(Self {
            config: std::sync::Arc::new(config),
            redis,
            db,
            adapters: Default::default(),
            adapter_generation: Default::default(),
            device_to_adapter: Default::default(),
            geofence,
            scripts,
            telemetry_tx,
            command_tx,
            event_tx,
            leases: Default::default(),
        })
    }
}
```

Redis 不可用时不能让进程启动失败：脚本改为懒加载（首次 acquire 时加载，失败则降级为单实例内存锁并告警），与 11.6 的降级矩阵一致。

drone-service 自建 `infra::redis::DroneRedis`（**不复用** rustcam-media 的 `RedisStore`/`RedisBus`，避免反向依赖），需实现：

- `script_load(script) -> String`（返回 SHA）
- `eval_sha(sha, keys, args) -> Value`
- `set_nx_ex(key, value, ttl_secs) -> bool`（设备独占占用）
- `xadd(stream, maxlen, fields)`
- `xreadgroup(group, consumer, streams, count, block_ms)`
- `xack(stream, group, ids)`
- `xpending(stream, group)`
- `xlen(stream)`
- 以及 `get/set/hset/hget/hgetall/expire/incr/sadd/srem/smembers/publish` 等基础命令。

`infra::db::DroneDb` 自建 rbatis 连接池（指向同一个 PostgreSQL，但只操作 `drone_*` 表），并提供 `run_migrations()` 执行 `migrations.sql`。

### 6.5 注册表与心跳

- Redis 键：
  - `adapter:instance:{adapter_id}`，TTL 30s，心跳续约。
  - `adapter:device:{drone_tag}`，TTL 30s，值为 `adapter_id`（字符串，用 `SET NX EX` 保证设备独占）。
- 注册：
  1. 校验 `adapter_id`、`protocol`、`version`。
  2. **按 `adapter_id` 查 `drone_device`**，取出该 adapter 名下的设备及其 `mavlink_addr` /
     `signing_secret` / `mavlink_sysid` / `mavlink_compid`（§12.4 L3）；库中查不到时本次不分配
     任何设备并记录告警（引导模式已取消，不再回落到 adapter 自报清单）。
  3. 对每个设备 `SET adapter:device:{id} adapter_id NX EX 30`（需新增 `set_nx_ex`）。
  4. 已被其他 adapter 占用的设备不分配，在 `RegisterAck.assigned_drone_tags` 中只返回成功的。
  5. 分配成功的设备连同端点与密钥经 `RegisterAck.assignments` 下发（§5.8）。
  6. 同一 `adapter_id` 重复注册：**新连接胜出**。`adapter_generation` 自增，旧连接在下行写入与上行处理前都校验 generation，不匹配则丢弃并关闭。
- 心跳：adapter 每 `heartbeat_interval_sec` 发 `Heartbeat`；service `EXPIRE` 续约实例键与设备键。
- 掉线：bidi 流结束或心跳超时 → 移除连接、清理设备映射、标记设备离线、发 `DeviceEvent(disconnected)`、经 EventService 广播。
- reaper：每 10s 扫描过期实例键，兜底清理。

> key 名统一来自 `src/redis_keys.rs`，禁止在业务代码里拼字符串。

### 6.6 gRPC AdapterLink

```rust
#[tonic::async_trait]
impl AdapterLink for AdapterLinkSvc {
    type ConnectStream =
        std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<ServiceMessage, Status>> + Send + 'static>>;

    async fn connect(
        &self,
        req: Request<tonic::Streaming<AdapterMessage>>,
    ) -> Result<Response<Self::ConnectStream>, Status> {
        let mut inbound = req.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(1024);

        let reg = match inbound.message().await? {
            Some(AdapterMessage {
                payload: Some(adapter_message::Payload::Register(r)),
            }) => r,
            _ => return Err(Status::invalid_argument("expected Register")),
        };

        let assigned = registry::register_adapter(
            &self.state, &reg.adapter_id, tx.clone(),
        ).await.map_err(|e| Status::internal(e.to_string()))?;

        tx.send(service_message::Payload::RegisterAck(RegisterAck {
            accepted: true,
            reason: String::new(),
            assigned_drone_tags: assigned,
            heartbeat_interval_sec: self.state.config.adapter_heartbeat_sec as i32,
        })).await.map_err(|_| Status::internal("send RegisterAck"))?;

        let state = self.state.clone();
        let adapter_id = reg.adapter_id.clone();
        let generation = state.next_generation(&adapter_id);

        tokio::spawn(async move {
            while let Ok(Some(msg)) = inbound.message().await {
                state.handle_adapter_message(&adapter_id, generation, msg).await;
            }
            registry::unregister(&state, &adapter_id, generation).await;
        });

        let stream = tokio_stream::StreamExt::map(
            tokio_stream::wrappers::ReceiverStream::new(rx),
            |p| Ok(ServiceMessage { payload: Some(p) }),
        );
        Ok(Response::new(Box::pin(stream)))
    }
}
```

`next_generation` 负责递增并返回该 adapter 的连接代次；`handle_adapter_message` 负责按 generation 过滤上行消息并分发到遥测/指令/事件处理。

上行消息处理：

| 消息 | 处理 |
|---|---|
| `Heartbeat` | 续约 Redis，更新实例状态 |
| `Telemetry` | 投递到 `telemetry_tx`，由消费者去重/影子/广播 |
| `CommandReply` | 更新指令状态机、审计、经 EventService 推送 |
| `Event` | 更新设备在线状态、告警、经 EventService 推送 |

下行消息：

| 消息 | 来源 |
|---|---|
| `RegisterAck` | 注册成功 |
| `Command` | `ControlService.SendCommand` 路由 |
| `ControlState` | 控制权变更/释放/抢占时广播（给 adapter 用于本地缓存与拒发提示）；对外事件走 `EventService` |

### 6.7 控制权仲裁

#### 控制类型

| `control_type` | 含义 | 典型动作 |
|---|---|---|
| `flight` | 飞行控制权 | 起飞、降落、返航、指点飞行 |
| `gimbal` | 云台控制权 | 转动、回中、变焦 |
| `mission` | 任务控制权 | 航线、任务启停 |

#### Redis 数据结构

```text
control:{drone_tag}:{control_type} = Hash {
  holder: operator,            -- claims.username（人可读；审计另存 user_id）
  priority: int,
  acquired_at: ms,
  expire_at: ms
}
TTL = lease_ms
```

#### 规则

- `acquire` 只在**无人持有或已过期**时成功；有人持有且未过期时一律失败（不按优先级抢占）。
- `preempt` 才允许按优先级抢占：请求优先级更高才成功，否则失败。
- 抢占时返回被抢占者 `preempted`，并写审计、广播 `ControlState`。
- 续约只允许当前 holder。
- 释放只允许当前 holder；管理员可强制释放（`force_release`，单独权限）。
- 默认租约 30s，持有方每 10s 续约。
- 过期由 TTL 自动生效；同时 service 在 `DroneState.leases`（内存 Hash）保留自己签发的 `holder/expire_at`，reaper 每秒扫描并对过期项广播 `expired` 事件、写审计——因为 key 随 TTL 消失，Redis 侧无法反查过期时刻。
- **Redis 不可用时**：进程启动不得失败；仲裁降级为**进程内锁**（`lease_lock` 保证单实例内的读-判断-写原子性）并打印告警。该降级态只保证单实例互斥，多实例部署必须保证 Redis 可用。

#### Lua：acquire

```lua
-- KEYS[1] = control:{drone_tag}:{control_type}
-- ARGV[1] = holder, ARGV[2] = priority, ARGV[3] = now_ms, ARGV[4] = lease_ms
local key    = KEYS[1]
local holder = ARGV[1]
local prio   = tonumber(ARGV[2])
local now    = tonumber(ARGV[3])
local lease  = tonumber(ARGV[4])

local cur = redis.call('HMGET', key, 'holder', 'priority', 'expire_at')
local cur_holder = cur[1]
local cur_expire = tonumber(cur[3] or '0')

-- 无人持有或已过期才允许获取；有人持有且未过期一律失败（抢占走 preempt.lua）
if not cur_holder or now >= cur_expire then
  redis.call('HSET', key,
    'holder', holder,
    'priority', prio,
    'acquired_at', now,
    'expire_at', now + lease)
  redis.call('PEXPIRE', key, lease)
  return {1, ''}
end

return {0, cur_holder}
```

#### Lua：release

```lua
-- KEYS[1], ARGV[1] = holder
local cur_holder = redis.call('HGET', KEYS[1], 'holder')
if cur_holder == ARGV[1] then
  redis.call('DEL', KEYS[1])
  return 1
end
return 0
```

#### Lua：renew

```lua
-- KEYS[1], ARGV[1] = holder, ARGV[2] = now_ms, ARGV[3] = lease_ms
local cur_holder = redis.call('HGET', KEYS[1], 'holder')
if cur_holder == ARGV[1] then
  redis.call('HSET', KEYS[1], 'expire_at', tonumber(ARGV[2]) + tonumber(ARGV[3]))
  redis.call('PEXPIRE', KEYS[1], ARGV[3])
  return 1
end
return 0
```

#### Lua：preempt

```lua
-- KEYS[1], ARGV[1] = holder, ARGV[2] = priority, ARGV[3] = now_ms, ARGV[4] = lease_ms
local key    = KEYS[1]
local holder = ARGV[1]
local prio   = tonumber(ARGV[2])
local now    = tonumber(ARGV[3])
local lease  = tonumber(ARGV[4])

local cur = redis.call('HMGET', key, 'holder', 'priority')
local old_holder = cur[1]
local old_prio   = tonumber(cur[2] or '0')

if not old_holder or prio > old_prio then
  redis.call('HSET', key,
    'holder', holder,
    'priority', prio,
    'acquired_at', now,
    'expire_at', now + lease)
  redis.call('PEXPIRE', key, lease)
  return {1, old_holder or ''}
end

return {0, old_holder}
```

#### Rust 调用

```rust
pub const ACQUIRE_LUA: &str = include_str!("lua/acquire.lua");
pub const RELEASE_LUA: &str = include_str!("lua/release.lua");
pub const RENEW_LUA:   &str = include_str!("lua/renew.lua");
pub const PREEMPT_LUA: &str = include_str!("lua/preempt.lua");

#[derive(Clone)]
pub struct LuaScripts {
    /// SHA1（`SCRIPT LOAD` 的返回值）
    pub acquire: String,
    pub release: String,
    pub renew: String,
    pub preempt: String,
    /// 源码同时保留：Redis 重启后 SHA 失效（NOSCRIPT）时用于重新加载
    pub acquire_src: &'static str,
    pub release_src: &'static str,
    pub renew_src: &'static str,
    pub preempt_src: &'static str,
}

impl LuaScripts {
    /// 启动时 SCRIPT LOAD 缓存 SHA1；Redis 重启后遇到 NOSCRIPT 用源码重载
    pub async fn load(redis: &crate::infrastructure::RedisStore) -> anyhow::Result<Self> {
        Ok(Self {
            acquire: redis.script_load(ACQUIRE_LUA).await?,
            release: redis.script_load(RELEASE_LUA).await?,
            renew: redis.script_load(RENEW_LUA).await?,
            preempt: redis.script_load(PREEMPT_LUA).await?,
            acquire_src: ACQUIRE_LUA,
            release_src: RELEASE_LUA,
            renew_src: RENEW_LUA,
            preempt_src: PREEMPT_LUA,
        })
    }

    /// Redis 不可用时的降级态：首次使用时懒加载
    pub fn lazy() -> Self { /* 省略：SHA 为空，首次调用时 load */ }
}

pub async fn acquire(
    state: &DroneState,
    sha: &str,
    key: &str,
    holder: &str,
    priority: i32,
    lease_ms: i64,
) -> anyhow::Result<LeaseResult> {
    let now = chrono::Utc::now().timestamp_millis();
    let (ok, preempted): (i32, String) = state.redis.eval_sha(
        sha,
        &[key],
        &[
            holder,
            &priority.to_string(),
            &now.to_string(),
            &lease_ms.to_string(),
        ],
    ).await?;
    Ok(LeaseResult { ok: ok == 1, preempted })
}
```

`script_load` / `eval_sha` 是 `DroneRedis` 需要实现的方法。`NOSCRIPT` 重载放在 drone 层：`LuaScripts` 同时保留源码，捕获 `NOSCRIPT` 后用源码重新 `script_load` 再重试一次；`eval_sha` 本身只负责执行。

落地要点：

- 启动时 `SCRIPT LOAD` 缓存 SHA，运行时用 `EVALSHA`。
- 捕获 `NOSCRIPT`，重新 `SCRIPT LOAD` 后重试一次。
- 时间戳从 Rust 传入，脚本内不使用 `TIME` 做写决策。
- key 通过 `KEYS[1]` 传，兼容 Redis Cluster。
- 脚本保持短小，禁止阻塞命令与长循环。
- 只对 acquire/release/renew/preempt 使用 Lua；普通 `HSET`/`GET`/`XADD` 直接调命令。

#### 审计

每次 acquire/release/preempt/expire 写 `drone_control_audit`：

```text
ts, drone_tag, control_type, action, holder, priority, result, operator
```

### 6.8 指令状态机

```text
                 ┌──────────┐
   SendCommand → │ pending  │
                 └────┬─────┘
                      │ 路由成功
                 ┌────▼─────┐
                 │  sent    │
                 └────┬─────┘
         adapter ack  │
                 ┌────▼─────┐
                 │  acked   │
                 └────┬─────┘
       执行成功/失败   │
        ┌─────────────┴─────────────┐
   ┌────▼─────┐               ┌─────▼────┐
   │ executed │               │  failed  │
   └──────────┘               └──────────┘

异常分支：超时 → timeout；校验失败/无控制权 → rejected
```

`command::run_worker` 从 `command_tx` 取任务，负责校验、路由、下发与审计；`command::timeout::run_reaper` 定期扫描超过 `deadline_ms` 的指令并标记 `timeout`。`CommandTask` 至少包含 `Command` 与来源 `operator`；如需同步等待最终回执，可附带 `oneshot::Sender<CommandReply>`，默认实现走异步 WS 回执。

流程：

1. 网关从 JWT 取 `operator`，不接受请求体伪造。
2. 校验 Casbin 权限与 `control:{drone_tag}:flight` 持有者（飞行类指令）。
3. 校验参数白名单与取值范围。
4. 生成/校验 `cmd_id`，`cmd:idem:{cmd_id}` 去重，TTL 24h。
5. 围栏预检（如 goto 目标点在禁飞区则拒绝）。
6. 查 `adapter:device:{drone_tag}` → `device_to_adapter` → adapter 下行通道。
7. 下发 `Command`，写入 `drone_command_audit`。
8. adapter 回 `CommandReply`，更新状态、审计、WS 推送。
9. `timeout` 扫描器定期将超过 `deadline_ms` 的 `pending/sent/acked` 标为 `timeout`。

指令类型与参数：

| type | 参数 | 说明 |
|---|---|---|
| `rtl` | 无 | 返航 |
| `takeoff` | `alt_m` | 起飞到相对高度 |
| `land` | 无 | 降落 |
| `goto` | `lat`, `lon`, `alt_m` | 指点飞行 |
| `gimbal` | `pitch_deg`, `yaw_deg`, `zoom` | 云台 |
| `camera` | `action`（start/stop/photo） | 相机 |

### 6.9 遥测处理

入站链路：

```text
adapter → bidi Telemetry → telemetry_tx(broadcast)
  → dedup(drone_tag + seq)
  → shadow 更新（Redis Hash）
  → geofence / risk 判断
  → event_tx 广播 → EventService → rustcam-server → 浏览器 WS
  → 可选 XADD telemetry:default（旁路，外部订阅）
  → XADD archive:telemetry → archive worker 批量写 TimescaleDB
```

去重：

- 每设备维护 `last_seq`（内存 DashMap，容量可控）。
- `seq` 回绕或重启时，若 `seq < last_seq - 1000` 视为新会话，重置。
- 重复包直接丢弃，不更新影子。

设备影子：

```text
shadow:{drone_tag} = Hash {
  ts_ms, lat, lon, alt_amsl, rel_alt,
  roll, pitch, yaw, battery_pct, voltage,
  link_quality, seq
}
TTL = 60s
```

`online` 不落 Redis（在线状态由 adapter 注册表 + `drone_device.online` 维护），避免影子 TTL 与注册表 TTL 打架。

广播：

- `tokio::sync::broadcast` 容量 4096。
- 慢消费者 `Lagged` 时记录告警并跳过，不阻塞主链路。
- WS 层按设备订阅过滤，避免全量推送。

### 6.10 围栏

MVP 表结构（普通 PostgreSQL 即可）：

```sql
CREATE TABLE IF NOT EXISTS drone_geofence (
  id          bigserial PRIMARY KEY,
  name        text NOT NULL,
  type        text NOT NULL,          -- no_fly / limited / return / work
  min_alt     double precision,
  max_alt     double precision,
  geojson     text NOT NULL,          -- GeoJSON Polygon / MultiPolygon
  enabled     boolean NOT NULL DEFAULT true,
  version     int NOT NULL DEFAULT 1,
  created_by  text,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_geofence_enabled ON drone_geofence (enabled);
```

> 这些语句在 `rustcam-drone-service` 的 `migrations.sql` 里，启动时由 `DroneDb::run_migrations()` 执行，必须幂等（`IF NOT EXISTS`）。

判断流程：

1. 启动/版本变化时从 DB 加载启用围栏，解析为 `geo::MultiPolygon`。
2. 遥测点先做 bbox 粗筛（可选 H3 cell 粗筛）。
3. 再做多边形 `contains` 精判（`geo` crate）。
4. 结合 `min_alt`/`max_alt` 判断高度违规。
5. 结果：
   - 进入 `no_fly` → `high` 告警。
   - 离开 `work` → `medium` 告警。
   - 低于 `min_alt` / 高于 `max_alt` → `medium` 告警。
6. 告警去抖：连续 2 个采样或持续 1s 才触发，离开后 3s 恢复。

> 围栏类型统一为 `no_fly` / `work` / `height`（`allowed` 是 `work` 的旧名，服务端归一化）。
> `height` 只做高度区间判定，不做平面包含判断。

版本刷新：

```text
围栏编辑 → POST/PUT /api/v1/drone/geofences
  → DB 写入，version + 1
  → Redis HSET geofence:version version {v} updated_at {ts}
  → 发布 Redis Stream 事件 geofence:updated
  → rustcam-drone-service 缓存刷新
```

规模化后迁移 PostGIS：

```sql
CREATE EXTENSION IF NOT EXISTS postgis;
ALTER TABLE drone_geofence ADD COLUMN IF NOT EXISTS geom geography(Polygon, 4326);
CREATE INDEX IF NOT EXISTS idx_drone_geofence_geom ON drone_geofence USING GIST (geom);
```

> 迁移前保持 `geojson` 字段为权威数据，`geom` 作为派生列，避免双写不一致。

### 6.11 风控

MVP 规则（`risk_enabled = true` 时启用）：

| 规则 | 条件 | 级别 |
|---|---|---|
| 低电量 | `battery_pct < 20` | medium |
| 严重低电量 | `battery_pct < 10` | high |
| 失联 | 超过 3 个心跳周期无遥测 | high |
| 超速 | 水平速度 > 配置阈值 | medium |
| 越界 | 进入 `no_fly` | high |
| 越界 | 离开 `work` / 高度越界 | medium |
| 高度异常 | 超出围栏限高或相对高度突变 | medium |

输出统一为 `Alert`，写 `risk:alerts` Stream 并经 EventService 推送。MVP 不做在线异常模型。

> 实现说明：告警当前走内存告警环 + EventService 广播 + `rustcam_drone_risk_alert_total{rule}`
> 指标；`risk:alerts` Stream 随 §8 的 Streams 落地后补齐。
> `risk_enabled = true` 时启用；超速阈值用 `risk_max_speed_mps` 配置（0 表示默认 30 m/s）。

### 6.12 HTTP API（由 rustcam-server 提供）

drone-service 不提供 HTTP。所有 REST 接口在 `rustcam-server` 侧实现，内部转成 gRPC 调用 drone-service：

| 方法 | 路径 | 说明 | 内部 gRPC |
|---|---|---|---|
| GET | `/api/v1/drone/ping` | 健康检查 | `QueryService.ListDevices` 或本地探活 |
| POST | `/api/v1/drone/ws-ticket` | 签发短时一次性 WS ticket | 本地生成（主服务侧） |
| GET | `/api/v1/drone/devices` | 设备列表 | `QueryService.ListDevices` |
| GET | `/api/v1/drone/devices/{drone_tag}` | 设备详情 | `QueryService.GetDevice` |
| GET | `/api/v1/drone/devices/{drone_tag}/telemetry` | 最新遥测（影子） | `QueryService.GetTelemetry` |
| POST | `/api/v1/drone/control/acquire` | 申请控制权（仅无人持有/已过期时成功） | `ControlService.AcquireControl` |
| POST | `/api/v1/drone/control/release` | 释放控制权 | `ControlService.ReleaseControl` |
| POST | `/api/v1/drone/control/renew` | 续约 | `ControlService.RenewControl` |
| POST | `/api/v1/drone/control/preempt` | 抢占（需更高优先级） |
| POST | `/api/v1/drone/control/force_release` | 强制释放（管理员） | `ControlService.ForceReleaseControl` |
| GET | `/api/v1/drone/control/{drone_tag}/{type}` | 查询控制权 | `ControlService.GetControlState` |
| POST | `/api/v1/drone/{drone_tag}/command` | 下发指令 | `ControlService.SendCommand` |
| GET | `/api/v1/drone/{drone_tag}/commands/{cmd_id}` | 查询指令状态 | `QueryService.GetCommandStatus` |
| GET | `/api/v1/drone/geofences` | 围栏列表 | `GeofenceService.ListGeofences` |
| POST | `/api/v1/drone/geofences` | 新建围栏 | `GeofenceService.CreateGeofence` |
| PUT | `/api/v1/drone/geofences/{id}` | 更新围栏 | `GeofenceService.UpdateGeofence` |
| DELETE | `/api/v1/drone/geofences/{id}` | 删除/禁用围栏 | `GeofenceService.DeleteGeofence` |
| GET | `/api/v1/drone/alerts` | 告警列表 | `QueryService.ListAlerts` |
| GET | `/api/v1/drone/replay` | 轨迹回放：`drone_tag`、`start_ms`、`end_ms`、`limit`（默认 5000；超限等步长降采样，按 `ts_ms` 升序返回） | `QueryService.GetTelemetryHistory` |

请求示例：

```json
POST /api/v1/drone/control/acquire
{
  "drone_tag": "drone-001",
  "control_type": "flight",
  "priority": 10,
  "lease_seconds": 30
}
```

```json
POST /api/v1/drone/drone-001/command
{
  "type": "goto",
  "params": { "lat": "31.2304", "lon": "121.4737", "alt_m": "50" },
  "deadline_ms": 1730000000000
}
```

响应统一使用现有 `api/response.rs` 的包装格式。

### 6.13 WebSocket（由 rustcam-server 提供）

drone-service 不提供 WS。`rustcam-server` 订阅 `EventService.Subscribe`，再按 `msg_type` 过滤后推给浏览器。

| 路径 | 内容 | 对应 msg_type |
|---|---|---|
| `/ws/drone/telemetry` | 遥测推送 | `telemetry` |
| `/ws/drone/alerts` | 告警 | `alert` |
| `/ws/drone/control` | 控制权变更 | `control` |
| `/ws/drone/commands` | 指令回执 | `command` |

主服务侧的转发链路：

```text
rustcam-server 启动 → 建 EventService 订阅（一条共享流）
  → 内部广播到本地 channel
  → 每个 /ws/drone/* 连接按自己的 msg_type 过滤后下发
```

drone-service 侧的广播源是 `DroneState.event_tx`（`tokio::broadcast`），不与现有 `WsBroadcaster` 混用，避免 drone 消息进入现有 `/ws`。

统一消息包与现有 `WsMessage` 保持一致，避免前端维护两套解析：

```json
{ "msg_type": "telemetry", "data": { }, "timestamp": "2026-11-03T12:00:00Z" }
{ "msg_type": "alert",     "data": { }, "timestamp": "2026-11-03T12:00:00Z" }
{ "msg_type": "control",   "data": { }, "timestamp": "2026-11-03T12:00:00Z" }
{ "msg_type": "command",   "data": { }, "timestamp": "2026-11-03T12:00:00Z" }
```

遥测：

```json
{
  "msg_type": "telemetry",
  "data": {
    "drone_tag": "drone-001",
    "lat": 31.2304,
    "lon": 121.4737,
    "alt_amsl": 62.5,
    "rel_alt": 50.0,
    "roll": 0.01,
    "pitch": -0.02,
    "yaw": 1.57,
    "battery_pct": 87.0,
    "link_quality": 96,
    "seq": 1024,
    "ts_ms": 1730000000000
  },
  "timestamp": "2026-11-03T12:00:00Z"
}
```

控制权：

```json
{
  "msg_type": "control",
  "data": {
    "drone_tag": "drone-001",
    "control_type": "flight",
    "holder": "user-42",
    "priority": 10,
    "expire_at_ms": 1730000030000,
    "action": "acquired"
  },
  "timestamp": "2026-11-03T12:00:00Z"
}
```

告警：

```json
{
  "msg_type": "alert",
  "data": {
    "drone_tag": "drone-001",
    "rule": "geofence",
    "level": "high",
    "message": "entered no_fly zone: airport-01"
  },
  "timestamp": "2026-11-03T12:00:00Z"
}
```

WS 连接要求：

- 浏览器 WS 不能设置 `Authorization` header，现有 `jwt_auth_layer` 只读 header，不能直接复用。
- 先调用 `POST /api/v1/drone/ws-ticket` 获取 60s 一次性 ticket，再连接 `?ticket=...`；服务端校验后升级，不把长期 access token 放 query。
- 客户端可发送订阅消息过滤 `drone_tags`。
- 服务端每 20s 发 ping；客户端断线指数退避重连（1s → 30s，带 jitter）。

通道与消息类型：

- drone **不使用**现有 `WsBroadcaster`，避免 drone 消息灌进现有 `/ws` 连接；drone-service 侧是 `DroneState.event_tx`，主服务侧再转成浏览器消息。
- 4 条 `/ws/drone/*` 路径共用一条主服务本地通道，按 `msg_type` 过滤后再下发：

| 路径 | 允许的 `msg_type` |
|---|---|
| `/ws/drone/telemetry` | `telemetry` |
| `/ws/drone/alerts` | `alert` |
| `/ws/drone/control` | `control` |
| `/ws/drone/commands` | `command` |

主服务侧的消息结构与现有 `WsMessage` 保持一致：

```rust
#[derive(Clone, serde::Serialize)]
pub struct DroneWsMessage {
    pub msg_type: String,          // telemetry / alert / control / command
    pub data: serde_json::Value,
    pub timestamp: String,         // RFC3339，与现有 WsMessage 一致
}
```

### 6.14 main.rs 改动

`rustcam-server`（主服务）只增加 drone client 与路由，控制面不在本进程：

```rust
use rustcam_media::drone_gateway::client::DroneClient;   // 放在 rustcam-media 内，网关侧

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ... 原有：config、tracing、watchdog、registry、auth 均不动

    // 1. 创建 drone-service 的 gRPC client（地址来自 config.toml 的 [drone]）
    let drone = DroneClient::connect(registry.infra.config().drone.service_addr.clone()).await?;

    // 2. DroneClient 注入 AppState，由 protected_routes 统一装配
    let app_state = AppState::from_registry(&registry).with_drone(drone);
    let app = create_router(app_state, auth_state);

    // ... 原有 HTTP/SIP/RTSP server 与 select 均不动
}
```

关键：**不要把 drone router 在 `create_router` 之后 `merge`**。正确做法是在 `src/api/http/routes.rs` 的 `protected_routes` 内 merge，确保被 `route_layer(jwt_auth_layer)` 覆盖；WS 路由单独注册，并在升级前校验 `ws-ticket`。

为此 `AppState` 需新增 `pub drone: Arc<DroneClient>` 与 `with_drone`，让 drone handler 能从 `State<Arc<FullState>>` 拿到 client；`DroneClient` 内部封装 `ControlService`/`QueryService`/`GeofenceService`/`EventService` 四个 tonic client。

主服务侧的配置只需要一个地址（主服务不持有任何控制面参数）：

```rust
// src/config.rs（主服务）
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, Default)]
pub struct DroneClientConfig {
    /// rustcam-drone-service 的 gRPC 地址
    #[serde(default = "default_drone_service_addr")]
    pub service_addr: String,
}

fn default_drone_service_addr() -> String { "http://127.0.0.1:50051".into() }

// AppConfig 中：
// #[serde(default)] pub drone: DroneClientConfig,
```

### 6.15 rustcam-drone-service 的对外接口

结论：**rustcam-drone-service 从第一天起就是独立进程，且不对外提供 HTTP；唯一对外入口是 rustcam-server 的 `/api/v1/drone/*`。**

理由：

- 前端只认 `/api/v1`（`request.js` baseURL），drone-service 另开端口要处理跨域、网关与两套鉴权。
- 鉴权收敛在 rustcam-server（JWT + Casbin），drone-service 再实现一遍会重复；drone-service 只信任内网调用方（生产用 mTLS）。
- 进程边界清晰：drone-service 可单独扩缩容与重启，不影响 SIP/媒体主链路。

调用链（固定形态，不再分阶段）：

```text
浏览器 ──HTTP/WS──> rustcam-server
                      │ gRPC(ControlService/QueryService/GeofenceService)
                      ▼
                rustcam-drone-service
                      ▲
                      │ gRPC(AdapterLink, bidi)
                rustcam-drone-adapter
```

约束：

- drone-service 的 gRPC 只监听内网，不对外网暴露；`AdapterLink` 面向 adapter，其余 service 面向 rustcam-server。
- drone-service 不做 JWT/Casbin 校验，信任来自内网的 rustcam-server；请求中携带 `operator`（由 rustcam-server 从 JWT 注入）。
- drone-service 暴露内网 `/metrics`（仅运维），与业务 HTTP 无关。
- 例外场景（RustCam 不在场的纯无人机部署）可直接对接 drone-service 的 gRPC，无需 HTTP。

---

## 7. rustcam-drone-adapter 详细设计

### 7.1 职责

- 启动即连 `rustcam-drone-service`，断线指数退避，永不退出。
- 维护 MAVLink/厂商连接池。
- 将协议数据规范化为 `Telemetry`、`CommandReply`、`DeviceEvent`。
- 执行下行 `Command`，返回回执。
- 断网时本地环形缓冲，恢复后补发（遥测可丢旧，事件不可丢）。
- 不做控制权、不做业务规则、不做围栏判断。

### 7.2 目录

```text
crates/rustcam-drone-adapter/src/
├── main.rs
├── config.rs
├── link/
│   └── mod.rs          # gRPC 会话：Register / 心跳 / 遥测与事件转发 / 指令分发 / 退避重连
├── fc/                 # flight controller（飞控）链路
│   ├── mod.rs          # 连接、读循环、GCS 心跳、遥测定时器、online 状态
│   ├── addr.rs         # MAVLink 连接串归一化（udp/tcp/serial、兼容 udp://）
│   ├── autopilot.rs    # PX4 / ArduPilot 识别、模式编码与名称
│   ├── state.rs        # MAVLink 消息 → 规范遥测（位置/姿态/电池/GPS/链路）
│   ├── telemetry.rs    # core Telemetry → gRPC Telemetry
│   └── command.rs      # Command → MAVLink 序列、COMMAND_ACK 回收
└── ...
```

当前实现范围：MAVLink 单机接入（PX4 / ArduPilot），UDP / TCP / 串口；DJI 与
C++ SDK sidecar 目录保留为后续厂商接入位。

多机接入（同 adapter 连多台无人机）是 §12.4 设备身份核验的**前置条件**：MAVLink 端点、
唯一标识、签名密钥都是 per-device 的，单连接实现无法表达。连接池落地前，§12.4 的 L3 核验
只能按"一 adapter 一机体"验证，多机场景必须等连接池就位后启用。

### 7.3 连接循环

```rust
pub async fn run(cfg: Config) -> anyhow::Result<()> {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));
    loop {
        match connect_once(&cfg).await {
            Ok(_) => backoff.reset(),
            Err(e) => {
                tracing::warn!(error = %e, "adapter link lost");
                tokio::time::sleep(backoff.next()).await;
            }
        }
    }
}

async fn connect_once(cfg: &Config) -> anyhow::Result<()> {
    let channel = Channel::from_shared(cfg.service_addr.clone())?
        .connect().await?;
    let mut client = AdapterLinkClient::new(channel);
    let (tx, rx) = mpsc::channel::<AdapterMessage>(1024);

    tx.send(AdapterMessage {
        payload: Some(adapter_message::Payload::Register(Register {
            adapter_id: cfg.adapter_id.clone(),
            protocol: cfg.protocol.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
        })),
    }).await?;

    tokio::spawn(heartbeat_loop(tx.clone(), cfg.clone()));
    tokio::spawn(telemetry_loop(tx.clone(), cfg.clone()));
    tokio::spawn(event_loop(tx.clone(), cfg.clone()));

    let mut inbound = client.open(ReceiverStream::new(rx)).await?.into_inner();
    while let Some(msg) = inbound.message().await? {
        match msg.payload {
            Some(service_message::Payload::RegisterAck(ack)) => apply_assignment(ack),
            Some(service_message::Payload::Command(cmd)) => {
                let reply = execute_command(cmd).await;
                tx.send(AdapterMessage {
                    payload: Some(adapter_message::Payload::CommandReply(reply)),
                }).await?;
            }
            Some(service_message::Payload::ControlState(s)) => apply_control_state(s),
            None => {}
        }
    }
    Ok(())
}
```

### 7.4 MAVLink 映射

| MAVLink 消息 | Telemetry 字段 |
|---|---|
| `HEARTBEAT` | `autopilot`（PX4=12 / ArduPilot=3）、`armed`、`mode`、在线状态 |
| `ATTITUDE` | `roll`, `pitch`, `yaw` |
| `ATTITUDE_QUATERNION` | `qx/qy/qz/qw`（MAVLink 的 q1..q4 = w,x,y,z） |
| `GLOBAL_POSITION_INT` | `lat`, `lon`, `alt_amsl`, `rel_alt`, `vx/vy/vz`、`yaw`（无 ATTITUDE 时用 hdg） |
| `GPS_RAW_INT` | `gps_fix`, `satellites`, `hdop`、经纬度/高度兜底 |
| `SYS_STATUS` / `BATTERY_STATUS` | `battery_pct`, `voltage` |
| `VFR_HUD` | 高度/航向兜底 |
| `RADIO_STATUS` | `link_quality`（RSSI 0..254 折算 0..100） |
| `HOME_POSITION` / `GPS_GLOBAL_ORIGIN` | `home_lat/home_lon/home_alt` |
| `AUTOPILOT_VERSION` | 固件版本（日志 / DeviceEvent） |
| `EXTENDED_SYS_STATE` | 着陆状态（DeviceEvent） |
| `COMMAND_ACK` | `CommandReply.status/code` |
| `STATUSTEXT` | `DeviceEvent(statustext)` |

模式名由适配层抹平：PX4 用 `custom_mode` 的 main/sub 两段（如 `AUTO.LOITER`），
ArduPilot 按机型翻译（Copter/Plane/Rover/Sub 的 `GUIDED`、`RTL`、`LOITER` 等）。

指令映射：

| Command.type | PX4 | ArduPilot |
|---|---|---|
| `rtl` | `MAV_CMD_NAV_RETURN_TO_LAUNCH` | 同左 |
| `takeoff` | `SET_MODE(AUTO_TAKEOFF)` → `MAV_CMD_NAV_TAKEOFF` | `SET_MODE(GUIDED)` → `MAV_CMD_NAV_TAKEOFF` |
| `land` | `MAV_CMD_NAV_LAND` | 同左 |
| `goto` | `MAV_CMD_DO_REPOSITION`（带 ACK） | `SET_MODE(GUIDED)` → `SET_POSITION_TARGET_GLOBAL_INT`（无 ACK，直接 accepted） |
| `gimbal` | `MAV_CMD_DO_MOUNT_CONTROL` | 同左 |
| `camera` | `MAV_CMD_IMAGE_START_CAPTURE` / `VIDEO_START_CAPTURE` / `VIDEO_STOP_CAPTURE` | 同左 |

模式切换统一用 `MAV_CMD_DO_SET_MODE`（`SET_MODE` 自 2015 年起已废弃）。

### 7.5 可靠性

- 遥测：允许丢弃旧数据，环形缓冲只保留最近 N 条。
- 事件：`connected/disconnected/link_lost/error` 不可丢，断网时持久化到本地队列（文件或 SQLite）。
- 指令回执：本地记录 `cmd_id`，重连后补发未确认回执。
- 重复注册：service 侧新连接胜出，adapter 侧收到流关闭后重新走注册流程。
- 心跳：adapter 每 10s 发 `Heartbeat`；连续 3 次发送失败视为链路断开。

### 7.6 配置

```toml
[adapter]
adapter_id = "adapter-mavlink-1"
protocol = "mavlink"
service_addr = "http://127.0.0.1:50051"
autopilot = "auto"                     # auto | px4 | ardupilot
source_system = 255
source_component = 190
reconnect_min_secs = 1
reconnect_max_secs = 30
telemetry_hz = 10
heartbeat_interval_secs = 10
heartbeat_timeout_ms = 5000
command_timeout_ms = 10000
buffer_size = 10000
```

多机接入（一个 adapter 同时连多台无人机）的**设备清单、端点与密钥不在 adapter.toml 里配**，
统一由平台数据库管理：录入 `drone_device` 后由 drone-service 按 `adapter_id` 分配并下发
（见 §12.4 L3）。adapter.toml 只保留进程级配置，示例见仓库根目录 `adapter.toml.example`。

约定：

- **引导模式已取消**：旧版的设备清单 / 设备 ID / MAVLink 端点等本地引导配置项已删除；
  adapter 上线只带 `adapter_id` + `service_addr` + 凭证，设备、端点与签名密钥全部从服务端
  `RegisterAck.assignments` 获得。
- 生产要求：`mavlink_addr` 由平台录入并绑定隧道内地址，禁止 `0.0.0.0`（§12.4 L1）。
- 端点、签名密钥、唯一标识三项均以 `drone_device` 表为唯一真相源，adapter 本地不持久化。

配置来源：默认读工作目录下的 `adapter.toml`，也支持环境变量覆盖（优先级：环境变量 > TOML）：

| 环境变量 | 对应字段 |
|---|---|
| `ADAPTER_ID` | `adapter_id` |
| `PROTOCOL` | `protocol` |
| `SERVICE_ADDR` | `service_addr` |
| `MAVLINK_AUTOPILOT` | `autopilot` |
| `TELEMETRY_HZ` | `telemetry_hz` |

生产环境使用 mTLS：adapter 只出站，不开放入站端口。**端点与签名密钥经该通道下发，
mTLS 因此是密钥分发的前置条件**（§12.2、§12.4）。

---

## 8. Redis 设计

### 8.1 Streams

| Stream | 生产者 | 消费者 | 说明 |
|---|---|---|---|
| `telemetry:default` | drone-service / adapter（旁路） | 外部订阅者 | 高频遥测旁路，`MAXLEN ~ 1e6`，**不用于写库** |
| `drone:events` | drone-service / adapter | events 消费者 | 上线/掉线/错误 |
| `cmd:reply` | adapter | command 模块 | gRPC 回执之外的兜底 |
| `risk:alerts` | drone-service | 告警 worker | 告警 |
| `archive:telemetry` | drone-service | archive worker | 唯一的遥测入库路径（写 TimescaleDB） |
| `geofence:updated` | rustcam-drone-service（围栏写入后发布） | rustcam-drone-service（各实例刷新缓存） | 围栏版本刷新 |

消费组：

- `archive` 组消费 `archive:telemetry`，批量写库。
- 使用 `XACK` + `XPENDING` 处理失败重试。
- 监控 `XPENDING` 增长，超过阈值告警。

### 8.2 Keys

```text
# 控制权（Hash + TTL）
control:{drone_tag}:{control_type} = {holder, priority, acquired_at, expire_at}

# adapter 实例（TTL 30s，心跳续约）
adapter:instance:{adapter_id} = {grpc_addr, protocol, device_count, last_hb}

# 设备 → adapter 映射（TTL 30s，字符串，SET NX EX 保证独占）
adapter:device:{drone_tag} = adapter_id

# 设备影子（最新遥测，TTL 60s）
shadow:{drone_tag} = {ts_ms, lat, lon, alt_amsl, rel_alt, roll, pitch, yaw, battery_pct, voltage, link_quality, seq}

# 指令幂等（TTL 24h）
cmd:idem:{cmd_id} = {status, ts_ms}

# 围栏版本（Hash）
geofence:version = {version, updated_at}
```

命名集中管理在 `src/redis_keys.rs`（唯一来源），禁止散落字符串拼接。

### 8.3 现有 RedisBus 缺口

当前 `RedisBus` 只提供 `get/set/hget/hset/hgetall/publish/expire/incr/sadd/srem/smembers` 等，没有 Lua 和 Streams。落地前必须补：

- `script_load` / `eval_sha`：控制权 acquire/release/renew/preempt。
- `set_nx_ex`：设备独占占用（`SET NX EX`）。
- `xadd` / `xreadgroup` / `xack` / `xpending` / `xlen` / `xrange`：遥测、事件、告警。
- `pexpire`：租约续期；也可复用现有 `expire` 的秒级版本。

`rustcam-drone-service` 自建 `DroneRedis`（不复用主服务的 `RedisBus`/`RedisStore`）。`redis 1.7.1` 已支持 `Script` 与 Streams，不需要新增依赖。

---

## 9. 数据库设计

所有 `drone_*` 表由 `rustcam-drone-service` 拥有，DDL 放在 `crates/rustcam-drone-service/migrations.sql`，启动时由 `DroneDb::run_migrations()` 幂等执行；主服务的 `sql/schema.sql` 与 `ensure_schema` 不感知这些表。

### 9.1 设备

```sql
CREATE TABLE IF NOT EXISTS drone_device (
  drone_tag     text PRIMARY KEY,
  protocol      text NOT NULL,          -- mavlink / dji-cloud / gimbal
  adapter_id    text,
  online        boolean NOT NULL DEFAULT false,
  last_seen     timestamptz,
  device_tag    text,                   -- 业务关联：现有 devices.device_tag
  channel_tag   text,                   -- 业务关联：现有 channels.channel_tag
  video_stream  text,                   -- 无人机相机视频流（走现有 RTSP/ZLMediaKit 链路）
  unique_id     text,                   -- 登记的无人机唯一标识（AUTOPILOT_VERSION.uid），见 §12.4
  mavlink_addr  text,                   -- 该机的 MAVLink 端点（隧道内地址），见 §12.4
  signing_secret text,                  -- 该机的 MAVLink 2 签名密钥（密文存储），见 §12.4
  signing_allow_unsigned boolean NOT NULL DEFAULT true,  -- 是否放行未签名帧（含 MAVLink 1 帧）
  mavlink_version smallint NOT NULL DEFAULT 2,
  mavlink_sysid   smallint NOT NULL DEFAULT 1,  -- 该机在归属 adapter 链路上的 system id
  mavlink_compid  smallint NOT NULL DEFAULT 1,
  meta          jsonb,
  created_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_drone_device_device_tag ON drone_device (device_tag);
CREATE INDEX IF NOT EXISTS idx_drone_device_channel_tag ON drone_device (channel_tag);
CREATE INDEX IF NOT EXISTS idx_drone_device_adapter_id ON drone_device (adapter_id);
-- 链路绑定：同一 adapter 内 sysid 唯一；adapter_id 为空或 sysid 未指定的行不参与约束。
CREATE UNIQUE INDEX IF NOT EXISTS uq_drone_device_adapter_sysid
    ON drone_device (adapter_id, mavlink_sysid)
    WHERE adapter_id <> '' AND mavlink_sysid > 0;
```

`device_tag` / `channel_tag` 是业务关联，不建外键：无人机视频通过现有 ONVIF/RTSP/ZLMediaKit 播放链路进入平台，drone-service 只负责云台/飞行控制权。

`unique_id` / `mavlink_addr` / `signing_secret` / `mavlink_sysid` 是机载链路的身份与连接参数
（§12.4）：无人机与其 MAVLink 链路同表，**不另起 link 表，也不引入 adapter 侧表**；
`(adapter_id, mavlink_sysid)` 唯一确定一条链路，`drone-service` 注册时按 `adapter_id` 查本表分配。

- `signing_secret` 存**密文**，主密钥来自 drone-service 的环境变量，不入库；**为空 = 该机
  不需要签名**（此时 `signing_allow_unsigned` 不生效）。
- `signing_allow_unsigned` 默认 `true`：混发 MAVLink 1/2 的飞控必须放行未签名帧，否则会丢掉
  全部 MAVLink 1 帧（§12.4 L2）；纯 MAVLink 2 且全部签名的飞控可置 `false` 收紧。
- 录入 `signing_secret` 时，人工在飞控侧设置**同一把**密钥（SOP，见 §12.4 L2）；平台与飞控
  两处必须同值，不一致即静默丢帧、链路不通。


### 9.2 控制审计

```sql
CREATE TABLE drone_control_audit (
  id           bigserial PRIMARY KEY,
  ts           timestamptz NOT NULL DEFAULT now(),
  drone_tag    text NOT NULL,
  control_type text NOT NULL,
  action       text NOT NULL,           -- acquire / release / preempt / expire
  holder       text,
  priority     int,
  result       text NOT NULL,
  operator     text
);
CREATE INDEX idx_drone_control_audit_device_ts
  ON drone_control_audit (drone_tag, ts DESC);
```

### 9.3 指令审计

```sql
CREATE TABLE drone_command_audit (
  id          bigserial PRIMARY KEY,
  ts          timestamptz NOT NULL DEFAULT now(),
  cmd_id      text NOT NULL UNIQUE,
  drone_tag   text NOT NULL,
  type        text NOT NULL,
  params      jsonb,
  status      text NOT NULL,
  operator    text,
  finished_at timestamptz
);
CREATE INDEX idx_drone_command_audit_device_ts
  ON drone_command_audit (drone_tag, ts DESC);
```

### 9.4 遥测历史（后置）

```sql
CREATE TABLE drone_telemetry (
  ts        timestamptz NOT NULL,
  drone_tag text NOT NULL,
  lat       double precision,
  lon       double precision,
  alt       double precision,
  roll      real,
  pitch     real,
  yaw       real,
  battery   real,
  link      int,
  seq       bigint
);
SELECT create_hypertable('drone_telemetry', 'ts');
CREATE INDEX ON drone_telemetry (drone_tag, ts DESC);
```

保留策略：原始数据 30 天；降采样 1Hz 长期表 1 年。

### 9.5 迁移策略

- MVP 只用 `drone_device`、`drone_control_audit`、`drone_command_audit`、`drone_geofence`。
- drone-service 启动时连 PostgreSQL 并执行自带 `migrations.sql`（幂等）；**建表失败必须报错**，不能静默继续（表不存在时功能会直接报错）。
- PostgreSQL 不可用时服务**仍可启动**：审计先写内存环（状态快照兜底），围栏用内存副本兜底；新编辑失败并提示用户。
- 现有库已启用 PostGIS（`sql/schema.sql` 与主服务 `ensure_schema` 都有），但 drone 表的 MVP 仍以 `geojson text` 为权威源，`geo` crate 做内存判断，避免 rbatis geometry 映射问题。
- 启用 PostGIS 后，`geom` 为派生列，由 `geojson` 生成，保持单一权威源。
- drone 表由 `rustcam-drone-service` 自带的 `migrations.sql` 创建：幂等 DDL（`CREATE TABLE IF NOT EXISTS`、`CREATE INDEX IF NOT EXISTS`），启动时执行；主服务不感知 drone_* 表。
- TimescaleDB 后置，不阻塞第一阶段。

---

## 10. 前端设计

### 10.1 技术栈

- Vue 3 + Vite 5 + Pinia + Vue Router + Element Plus。
- 2D：`maplibre-gl` + `terra-draw` + `terra-draw-maplibre-gl-adapter`（已有）。
- 3D：新增 `cesium` + `vite-plugin-cesium`。
- 语言：JavaScript + JSDoc + `// @ts-check`。
- 校验：手写 `isTelemetry`/`isAlert`，复杂消息可用 `zod`。

### 10.2 路由

```text
/drone/live        实时地图（2D/3D）
/drone/geofence    围栏编辑
/drone/control     分控面板
/drone/alerts      告警
/drone/replay      轨迹回放
/drone/devices     设备列表
```

`frontend/src/router/index.js` 在现有 `AppLayout` 的 children 里追加懒加载路由（沿用现有平铺写法，不需要新增 `DroneLayout`）：

```js
// 追加到根路由（component: AppLayout.vue）的 children 数组
{ path: 'drone/live',     component: () => import('../views/drone/LiveView.vue') },
{ path: 'drone/geofence', component: () => import('../views/drone/GeofenceView.vue') },
{ path: 'drone/control',  component: () => import('../views/drone/ControlView.vue') },
{ path: 'drone/alerts',   component: () => import('../views/drone/AlertView.vue') },
{ path: 'drone/replay',   component: () => import('../views/drone/ReplayView.vue') },
{ path: 'drone/devices',  component: () => import('../views/drone/DeviceView.vue') }
```

### 10.3 目录

```text
frontend/src/
├── api/
│   ├── drone.js                 # REST 封装
│   └── droneWs.js               # WS 连接/重连
├── stores/
│   ├── droneDevices.js          # 设备状态
│   ├── droneTelemetry.js        # 遥测缓存
│   ├── droneControl.js          # 控制权
│   └── droneAlerts.js           # 告警
├── views/drone/
│   ├── LiveView.vue
│   ├── GeofenceView.vue
│   ├── ControlView.vue
│   ├── AlertView.vue
│   ├── ReplayView.vue
│   └── DeviceView.vue
├── components/drone/
│   ├── Map2D.vue
│   ├── Map3D.vue
│   ├── GeofenceDraw.vue
│   ├── DroneEntity.vue
│   ├── ControlPanel.vue
│   ├── CommandPanel.vue
│   ├── TelemetryPanel.vue
│   └── AlertList.vue
├── composables/
│   ├── useDroneWs.js             # WS 连接/重连（与现有 useWsConnection.js 同级）
│   └── useGeofence.js            # 围栏编辑状态
└── utils/
    ├── droneValidate.js          # 消息运行时校验
    └── droneGeo.js               # 几何辅助
```

### 10.4 WebSocket 封装

```js
// @ts-check
import { request } from '../utils/request';

/**
 * @typedef {Object} Telemetry
 * @property {string} drone_tag
 * @property {number} lat
 * @property {number} lon
 * @property {number} alt_amsl
 * @property {number} rel_alt
 * @property {number} roll
 * @property {number} pitch
 * @property {number} yaw
 * @property {number} battery_pct
 * @property {number} link_quality
 * @property {number} seq
 * @property {number} ts_ms
 */

/**
 * @param {(t: Telemetry) => void} onTelemetry
 * @returns {{ close: () => void }}
 */
export function connectTelemetry(onTelemetry) {
  let ws;
  let retry = 0;
  let closed = false;

  async function open() {
    // 浏览器 WS 不能带 Authorization header，先换取一次性 ticket
    const res = await request.post('/drone/ws-ticket');
    const ticket = res.data.data.ticket;

    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(
      `${proto}://${location.host}/ws/drone/telemetry?ticket=${encodeURIComponent(ticket)}`
    );

    ws.onopen = () => { retry = 0; };

    ws.onmessage = (ev) => {
      let msg;
      try { msg = JSON.parse(ev.data); } catch { return; }
      if (msg.msg_type === 'telemetry' && isTelemetry(msg.data)) {
        onTelemetry(msg.data);
      }
    };

    ws.onclose = () => {
      if (closed) return;
      const delay = Math.min(1000 * 2 ** retry++, 30000) + Math.random() * 1000;
      setTimeout(open, delay);
    };

    ws.onerror = () => ws.close();
  }

  open();
  return { close() { closed = true; ws && ws.close(); } };
}

/**
 * @param {any} t
 * @returns {boolean}
 */
export function isTelemetry(t) {
  return !!t
    && typeof t.drone_tag === 'string'
    && Number.isFinite(t.lat)
    && Number.isFinite(t.lon)
    && Number.isFinite(t.alt_amsl)
    && Number.isFinite(t.roll)
    && Number.isFinite(t.pitch)
    && Number.isFinite(t.yaw)
    && Number.isFinite(t.ts_ms);
}
```

### 10.5 terra-draw 围栏

```js
import { TerraDraw, TerraDrawPolygonMode } from 'terra-draw';
import { TerraDrawMapLibreGLAdapter } from 'terra-draw-maplibre-gl-adapter';

/**
 * @param {import('maplibre-gl').Map} map
 * @param {(geojson: any) => void} onSave
 */
export function initGeofenceDraw(map, onSave) {
  const draw = new TerraDraw({
    adapter: new TerraDrawMapLibreGLAdapter({ map }),
    modes: [new TerraDrawPolygonMode()],
  });

  draw.start();
  draw.on('finish', (id) => {
    const feature = draw.getSnapshotFeature(id);
    if (feature && feature.geometry) onSave(feature.geometry);
  });

  return draw;
}
```

### 10.6 Cesium 姿态

```js
// @ts-check
import * as Cesium from 'cesium';

/**
 * @param {Cesium.Viewer} viewer
 * @param {string} deviceId
 */
export function createDroneEntity(viewer, deviceId) {
  return viewer.entities.add({
    id: deviceId,
    model: {
      uri: '/models/drone.glb',
      minimumPixelSize: 64,
      maximumScale: 200,
    },
    position: Cesium.Cartesian3.fromDegrees(0, 0, 0),
    orientation: Cesium.Quaternion.IDENTITY,
  });
}

/**
 * @param {Cesium.Entity} entity
 * @param {import('../../api/v1/droneWs').Telemetry} t
 */
export function updateDroneEntity(entity, t) {
  const pos = Cesium.Cartesian3.fromDegrees(t.lon, t.lat, t.alt_amsl);
  const hpr = new Cesium.HeadingPitchRoll(t.yaw, t.pitch, t.roll);
  entity.position = pos;
  entity.orientation = Cesium.Transforms.headingPitchRollQuaternion(pos, hpr);
}
```

`vite.config.js`：

```js
import cesium from 'vite-plugin-cesium';

export default defineConfig({
  plugins: [vue(), cesium()],
});
```

### 10.7 前端约束

- REST 走现有 `request`（baseURL 已是 `/api/v1`），`api/drone.js` 里写 `/drone/...`，不要再写 `/api/v1/drone/...`。
- `/drone/*` 路由挂在现有 `AppLayout` 的 children 下并带 `meta: { requiresAuth: true }`，与现有页面一致。
- `/drone/live` 路由懒加载，Cesium 单独 chunk，避免影响现有页面首屏。
- 遥测按 `drone_tag` 过滤，避免全量渲染。
- 控制权按钮必须显示当前 holder、优先级、剩余租约。
- 指令下发前二次确认，显示目标点/高度/返航点。
- 所有 WS/REST 消息先校验再入 store，避免异常数据导致页面崩溃。
- 不在前端做围栏合法性判断的最终裁决，只做提示；服务端为准。

---

## 11. 端到端流程

### 11.1 adapter 启动

```text
adapter 启动
  → 读取配置（adapter_id / protocol / service_addr）
  → gRPC 连接 rustcam-drone-service:50051（生产 mTLS）
  → bidi Connect，首帧 Register
  → service 校验 + Redis SET NX 分配设备
  → RegisterAck 返回 assigned_drone_tags + assignments + heartbeat_interval_sec
  → adapter 为每台设备建 MAVLink 连接，随后上报 IdentityReport
  → 心跳 10s、遥测上行、指令下行
  → 断线：指数退避重连，重连后重新 Register
```

### 11.2 遥测上行

```text
PX4/ArduPilot
  → adapter 解析 ATTITUDE / GLOBAL_POSITION_INT / SYS_STATUS
  → 规范化 Telemetry
  → gRPC bidi 上行
  → drone-service 去重(seq)
  → shadow 更新
  → 围栏/风控判断
  → EventService 事件流 → rustcam-server → WS 推送前端
  → 可选 XADD telemetry:default（旁路）
  → XADD archive:telemetry → archive worker → TimescaleDB
```

### 11.3 分控

```text
前端 → POST /api/v1/drone/control/acquire
  → rustcam-server 校验 JWT + Casbin，注入 operator
  → gRPC ControlService.AcquireControl
  → Redis Lua：读 holder/expire → 判断 → 写入（抢占走 preempt 接口）
  → 成功：返回 Lease + 经 EventService 广播 control 事件
  → 失败：返回 409 + 当前 holder
  → 持有方每 10s renew，TTL 30s
  → 断线/超时 → 自动过期 + WS 通知
```

### 11.4 指令下发

```text
前端 → POST /api/v1/drone/{id}/command
  → rustcam-server 校验 JWT/Casbin，注入 operator
  → gRPC ControlService.SendCommand
  → drone-service 校验控制权（flight 类指令）
  → 校验参数白名单
  → 幂等检查 cmd_id
  → 围栏预检
  → 查 adapter:device:{drone_tag}
  → bidi 下行 Command(deadline_ms)
  → adapter → MAVLink → 飞控
  → COMMAND_ACK → adapter → CommandReply
  → 状态机更新 + 审计 + EventService 推送
  → 超时未回执 → timeout
```

### 11.5 围栏

```text
前端 terra-draw 绘制 → GeoJSON
  → POST/PUT /api/v1/drone/geofences（rustcam-server）
  → gRPC GeofenceService → drone-service 写 DB，version + 1
  → Redis geofence:version 更新
  → 发布 Redis Stream 事件 geofence:updated
  → drone-service 缓存重载
  → 遥测：bbox 粗筛 → geo 精判 → 越界告警 → EventService → WS
```

### 11.6 故障降级矩阵

| 故障 | 行为 |
|---|---|
| adapter 断链 | service 心跳超时 → 设备离线 → 控制权到期释放 → 告警；adapter 退避重连 |
| service 重启 | adapter 重连 → 重新 Register → 重新分配；控制权从 Redis 恢复/过期 |
| drone-service 不可用 | rustcam-server 的 drone REST 返回 503（明确错误，不静默）；主服务 SIP/媒体不受影响 |
| Redis 挂 | service 本地队列缓冲，恢复后补发；控制权降级为单实例内存锁并告警 |
| PostgreSQL 挂 | 围栏用内存副本兜底；新编辑失败并提示；审计写本地重试队列 |
| 飞控掉线 | adapter 检测 MAVLink 心跳丢失 → `DeviceEvent(disconnected)` |
| 重复 adapter 注册 | 新连接胜出，旧连接 generation 失效并关闭 |
| WS 慢消费者 | broadcast `Lagged`，丢弃旧消息并记录指标，不阻塞主链路 |
| 指令无回执 | deadline 到期标 `timeout`，审计并 WS 通知 |

---

## 12. 安全与权限

### 12.1 身份与权限

- REST 复用现有 JWT（`jwt_auth_layer` 读 `Authorization: Bearer`）。
- 身份口径统一：`operator`/`holder` 用 `claims.username`（人可读、可用于审计与 holder 比较）；`claims.sub`（UUID）作为审计里的 `user_id`。JWT 里的 `sub` 必须是 UUID，不能直接当 `user-42` 这类 holder 值用；请求体一律不覆盖。
- WS 不能直接复用 `jwt_auth_layer`（浏览器无法设置 header），改用 `POST /api/v1/drone/ws-ticket` 换取 60s 一次性 ticket。
- Casbin 只保留模型（`casbin_model.conf` 的 matcher/effect），**策略不再使用 CSV 文件，全部来自数据库表**：
  - `roles`：角色，`roles.name` 与 `enforce` 的第一个参数完全一致（现有种子角色名是 `Admin`）。
  - `permissions`：权限点，`permissions.name` 采用 `obj:act` 约定，如 `drone:control:acquire`；无冒号时按 `*:act` 处理。
  - `role_permissions`：角色与权限点的多对多关系。
  - `role_inheritance`：角色继承关系（`parent_role_id` → `child_role_id`），加载为 Casbin `g, child, parent`，子角色自动获得父角色的全部权限，支持多级继承。
  - `user_roles`：用户与角色的多对多关系；用户可能有多个角色，实现上按角色逐个 `enforce`，任一命中即放行。
- `CasbinManager` 启动时从数据库加载全量策略，角色或权限变更后调用 `reload_from_db` 刷新内存策略；数据库加载失败时以空策略启动并告警，不阻塞服务。
- 策略重载是原子的：先在新的 Enforcer 上构建完整策略（含 `p` 规则与 `g` 继承规则）并校验，通过后整体替换；校验或构建失败时保留原有策略，不会出现半套策略。
- 角色继承不允许成环：设置继承关系前做环检测，成环直接拒绝（400），避免角色链无限展开。
- 权限名在加载时校验（非空、无空白、对象与动作均非空），非法权限名会让整次重载失败并保留旧策略，避免拼写错误被静默忽略。
- 角色/权限/继承关系的管理接口（`/api/v1/roles`、`/api/v1/permissions`、`/api/v1/roles/{id}/parents`）在变更后立即刷新内存策略；权限校验使用 `role:manage`，用户管理使用 `user:read` / `user:write`。
- drone 权限需要在 `permissions` 中补权限点，并通过 `role_permissions` 授权给角色。权限名按**首个冒号**拆成 `(obj = "drone", act = "后缀")`，例如 `drone:control:acquire` → `enforce(role, "drone", "control:acquire")`：
  - `drone:view`（查看无人机）、`drone:telemetry:read`（读取遥测）、`drone:control:acquire`（获取/续约控制权）、`drone:control:preempt`（抢占）、`drone:control:release`（释放）、`drone:control:force_release`（强制释放）、`drone:command:send`（下发指令）。
  - `drone:manage`（管理无人机档案、航线与围栏）：规则里的围栏 CRUD 属于安全相关写操作，必须与只读查看分开授权，因此在上述清单外补充该权限点。

```sql
INSERT INTO permissions (id, name, description) VALUES
  (gen_random_uuid(), 'drone:view',              '查看无人机'),
  (gen_random_uuid(), 'drone:telemetry:read',    '读取遥测'),
  (gen_random_uuid(), 'drone:control:acquire',   '获取控制权'),
  (gen_random_uuid(), 'drone:control:preempt',   '抢占控制权'),
  (gen_random_uuid(), 'drone:control:release',   '释放控制权'),
  (gen_random_uuid(), 'drone:control:force_release', '强制释放控制权'),
  (gen_random_uuid(), 'drone:command:send',      '下发指令'),
  (gen_random_uuid(), 'drone:manage',            '管理无人机档案、航线与围栏');

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
JOIN permissions p ON p.name IN (
  'drone:view',
  'drone:telemetry:read',
  'drone:control:acquire',
  'drone:control:preempt',
  'drone:control:release',
  'drone:control:force_release',
  'drone:command:send',
  'drone:manage'
)
WHERE r.name = 'Admin';
```

> 权限点与授权由 `sql/migrate_drone_permissions.py` 幂等脚本落库；策略改动后需调用 `reload_from_db`（角色/权限管理接口已自动触发）。

- 无人机路由的鉴权由 rustcam-server 侧的 `route_layer` 中间件统一完成，**先于请求体解析**：
  - 路由 → 权限点的映射集中在一处（`crates/rustcam-media/src/drone_gateway/permissions.rs`），未登记权限点的路由一律拒绝（403），避免新增路由漏配而裸奔。
  - 无 token → 401；已认证但权限不足 → 403；不再存在「取不到用户就回落为 admin」的兜底。
  - WS 票据在签发时绑定可订阅的消息类型范围（`drone:view` → 告警/控制权/指令回执，`drone:telemetry:read` → 遥测），升级时按路径校验，越权订阅返回 403。
- 设备级权限：若现有 model 支持 `drone_tag` 维度，按 `operator + drone_tag + action` 校验；否则先按角色，后续扩展。
- 存量库需要执行 `sql/migrate_role_inheritance.sql` 幂等脚本创建 `role_inheritance` 表与相关索引；表缺失时角色继承按空处理并告警，不影响其余权限。

### 12.2 gRPC 安全

- 生产启用 mTLS：`rustcam-drone-service` 验证 adapter 与 rustcam-server 的客户端证书。
- adapter 只出站连 drone-service，不监听入站端口。
- `rustcam-server → rustcam-drone-service` 的 gRPC 仅内网可达，`operator` 由 rustcam-server 从 JWT 注入并在 gRPC 中透传，drone-service 不重复做 JWT/Casbin。
- 证书 bootstrap 与轮换独立于业务；证书到期前告警。
- gRPC 端口不对外网暴露，仅容器网络/内网可达。

### 12.3 指令安全

- 指令类型白名单，未知类型直接 `rejected`。
- 参数范围校验：经纬度、高度、云台角度、变焦倍率。
- `goto` 必须做围栏预检；禁飞区内直接拒绝。
- 指令审计不可省略，记录 operator、cmd_id、参数、结果。
- 高风险指令（flight 类：takeoff / land / goto / rtl）要求持有 `flight` 控制权。

### 12.4 MAVLink 链路身份与加密（adapter ↔ 飞控）

§12.2 的 mTLS 只覆盖 `adapter ↔ drone-service` 这一跳（TCP/gRPC）。本节处理
`adapter ↔ 飞控` 这一跳：它跑在 UDP/串口上，飞控（PX4 / ArduPilot）不提供 TLS 服务端，
**mTLS 在这一跳无法直接使用**。这一跳拆成三层：L1 是部署要求，L2 与 L3 需要本仓库写代码。

威胁模型（部署形态：adapter 与飞控异机、跨网段/跨机）：

| 目标 | 归属 |
|---|---|
| 外部主机不能直接连上飞控下发指令 | L1 |
| 隧道内其他主机不能冒充飞控、不能注入有效指令 | L2 |
| 平台内不错控、串机、未登记机体占用设备号 | L3 |

#### L1 网络层：隧道 + 暴露面收敛（部署要求，无代码）

- WireGuard 隧道：机载端与 adapter 端各一个接口，**公钥即设备身份**（可登记、可吊销）。
- 飞控的 MAVLink 端口只绑隧道网段地址（如 `10.88.0.x`），防火墙只放行隧道接口。
- 端点由平台录入 `drone_device.mavlink_addr`（隧道内地址），**禁止 `0.0.0.0`**：当前默认
  `udpin:0.0.0.0:14540` 等于把飞控端口开给所有网卡，任何能到达该网段的主机都能直接
  给飞控下指令，完全绕过平台。
- 备选：加密数传电台；用 4G 专网（APN/VPDN）承载隧道。
- 不用 mTLS 隧道代理的原因：代理方案要求机载端额外跑代理进程并自建证书分发体系，飞控侧
  还要加装硬件；WireGuard 只需飞控配一个网段地址，两端公钥天然是设备身份。

#### L2 协议层：MAVLink 2 消息签名（可选加固，代码量小）

**密钥可选**：签名是 MAVLink 2 的能力，MAVLink 1 帧格式里没有签名字段（V1 帧 = STX / len /
seq / sysid / compid / msgid / payload / checksum；V2 才有 incompat_flags 与 13 字节签名）。
纯 MAVLink 1 的链路不配密钥。**服务器口径：`drone_device.signing_secret` 为空 = 该机不需要
签名**；非空则启用签名并按设备下发。

**对称机制**：同一把 32 字节 secret 在飞控与 adapter 各存一份，两端各签各验。

- 飞控侧一机一密钥：ArduPilot 用 `MAV2_SIGNING_SECRET_KEY`（旧版固件为 `MAV_SIGNING_SECRET_KEY`）
  配合 `MAV2_SIGNING_TIMESTAMP`（自动更新时间戳）；PX4 同样支持 MAVLink 2 消息签名，参数名按固件
  版本确认。
- 强制模式：两端都能要求"只收已签名帧"——飞控侧开启只接受已签名入站消息，adapter 侧
  `allow_unsigned = false`，此时链路为**双向强制签名**（最强组合，见下方组合表最后一行）。
- 密钥进入飞控的方式：**人工在飞控侧设置同一把密钥**（地面站参数或参数文件），SOP 化。
  平台录入无人机时填同一把值（§9.1 `signing_secret`），录入环节本来就是人肉，可接受；
  不做"平台自动写飞控参数"，避免依赖固件是否允许经 `PARAM_SET` 写内部参数。
- adapter 侧：workspace 的 `mavlink` 开 feature `mav2-message-signing`，建链后
  `conn.setup_signing(Some(SigningConfig::new(key, link_id, sign_outgoing, allow_unsigned)))`
  → 出站签名、入站验签。密钥由服务端按设备分配下发（`RegisterAck.assignments[].signing_secret`，
  §5.8），一机一密钥。
- 出站消息必须**自行置 `IFLAG_SIGNED`**：`sign_message` 只在该标志已置时才计算签名，
  否则该调用是空操作，对端看到未签名帧照样丢弃。
- 效果：隧道内其他主机伪造不出已签名的飞控帧，也注入不了有效的已签名指令；签名带时间戳
  窗口，可防重放。
- 代价：**不加密内容**，遥测仍可读；密钥轮换即需同时改飞控与平台两处。

`allow_unsigned` 的取值决定"未签名帧放不放行"，而 **MAVLink 1 帧在实现里与未签名的
MAVLink 2 帧走同一分支**（V1 → `allow_unsigned`；V2 带签名 → 验签；V2 未签名 → `allow_unsigned`）：

| 飞控发出的帧 | `allow_unsigned` | 结果 |
|---|---|---|
| 纯 MAVLink 2 且全部签名 | `false`（最强） | 未签名帧一律丢弃 |
| 混发 MAVLink 1 + MAVLink 2 | **必须 `true`** | MAVLink 1 照收；已签名的 MAVLink 2 仍验签；**未签名的 MAVLink 2 帧也会被放行** |
| 纯 MAVLink 1 | 不配密钥 | 无签名可用 |

**MAVLink 2 帧本身就可以不签名**：帧里 `IFLAG_SIGNED` 未置时就不带那 13 字节签名。
因此"平台配了密钥"不等于"链路有防护"——飞控侧不配签名就不会产生签名帧，组合关系如下：

| 飞控侧 | adapter 侧 | 实际效果 |
|---|---|---|
| 未配签名 | 未配密钥 | 无签名（默认行为） |
| 未配签名 | 配密钥 + `allow_unsigned=true` | 等同无签名：收不到任何已签名帧，防护为零 |
| 未配签名 | 配密钥 + `allow_unsigned=false` | **一帧都收不到**（全部未签名 → 全丢），表现为链路不通 |
| 配签名（同值） | 未配密钥 | adapter 不验签 → 防护只取决于飞控侧是否拒收未签名的入站帧 |
| 配签名（同值） | 配密钥 + `allow_unsigned=true` | 已签名帧验签通过才收，未签名帧照收 → 防护部分生效 |
| 配签名（同值） | 配密钥 + `allow_unsigned=false` | 最强：只收验签通过的帧（配合飞控侧"只收已签名入站消息"即双向强制签名） |

- adapter 必须兼容 v1/v2 混合流（`fc/mod.rs` 注明新旧固件可能同时存在两种帧），因此不能
  一律写死 `allow_unsigned = false`：那会在老固件链路上丢掉所有 MAVLink 1 帧。
- 取值按链路配置（`drone_device.signing_allow_unsigned`，默认 `true` 保兼容）；**L2 是条件性
  加固，不是"开了就安全"**：混发链路上的签名防护只覆盖"已签名的帧"，真正的防线仍是 L1 隧道。
- 想上"双向强制签名"，前提是飞控固件发出来的就是 MAVLink 2 且已签名——纯 MAVLink 1 或
  混发且未签名的链路做不到，只能退回 `allow_unsigned = true` 并依赖 L1 隧道。

必须同时一致的项（任一不一致都**静默丢帧 → 链路不通，且无报错**）：

| 项 | 不一致的后果 |
|---|---|
| `secret_key` | 验签失败，所有已签名帧被丢 |
| `link_id` | 签名计算含 link_id，一样全丢 |
| 时间戳 | 同流内时间戳必须严格递增；飞控侧必须开 `MAV2_SIGNING_TIMESTAMP`，否则时间戳不动 → 被拒 |

排障与降级：

- 开启签名后必须观测两个计数：**收到的帧数**与**验签通过的帧数**。两者不等即说明密钥 /
  link_id / 时间戳三者之一不匹配（先查这三项，不要先怀疑网络）。
- `signing_secret` 为空表示该机**不需要签名**：adapter 不调用 `setup_signing`，验签全部跳过，
  行为与现状一致（§5.8、§9.1）。
- 前提：消息签名要求 MAVLink 2；固件只发 MAVLink 1 时该链路退化为不签名。

#### L3 平台层：数据库驱动的端点分配与唯一标识核验（本仓库代码改动）

##### 标识模型与「引导模式取消」

平台用三个标识各司其职：

| 标识 | 产生方 | 角色 |
|---|---|---|
| `drone_tag` | 平台建档时生成 `drone-<uuid4>` | 平台主键、对外标识；DB / REST / 前端 / Redis / proto / 审计统一使用 |
| `unique_id` | 飞控 `AUTOPILOT_VERSION.uid` | 物理身份锚点；档案登记值与 adapter 实测值比对 |
| `(adapter_id, mavlink_sysid)` | 平台建档时指定 + 飞控实测 | 链路绑定：这条链路属于哪台机体；同一 adapter 内唯一 |

运维影响：**引导模式已取消**。adapter 配置里不再有设备清单、设备 ID 与 MAVLink 端点；
adapter 启动后若在 `drone_device` 查不到 `adapter_id` 名下的档案，就不会接管任何设备。
首次上线必须先在后端建档（`unique_id` / `adapter_id` / `mavlink_sysid` / `mavlink_addr`），
再启动 adapter 等待 `RegisterAck.assignments`。

端点与签名密钥同样落库，由平台分配给 adapter（§9.1、§5.8）。

数据模型（**drone 表 `drone_device` 一张表装下无人机与其链路**，不另起 link 表，也不引入
任何 adapter 侧表——adapter 只是分配的消费者）：

| 列 | 含义 |
|---|---|
| `drone_tag` | 平台生成的无人机标识（主键，不可变） |
| `unique_id` | 登记的无人机唯一标识（对应 `AUTOPILOT_VERSION.uid`） |
| `adapter_id` | 该机归属的 adapter（决定谁拿到分配） |
| `mavlink_sysid` / `mavlink_compid` | 期望的飞控 system/component id；`(adapter_id, mavlink_sysid)` 唯一 |
| `mavlink_addr` | 该机的 MAVLink 端点（隧道内地址） |
| `signing_secret` | 该机的 MAVLink 2 签名密钥（密文存储，见下） |

流程：

| 环节 | 动作 |
|---|---|
| 录入 | 平台「添加无人机」表单填 `unique_id` / `adapter_id` / `mavlink_sysid` / `mavlink_addr` / `signing_secret`（可选）→ `POST /api/v1/drone/drones` → drone-service 生成 `drone_tag` 并落 `drone_device` |
| 分配 | adapter 注册时，drone-service 按 `adapter_id` 查 `drone_device`，结合 Redis 设备独占（`SET NX EX`）得出该 adapter 本次实际接管的设备 → 经 `RegisterAck.assigned_drone_tags` + `assignments` 下发 `drone_tag`、端点与密钥 |
| 建链 | adapter 拿到 assignments 后逐条建 MAVLink 连接（**先注册后连飞控**） |
| 上报 | adapter 从飞控读 `AUTOPILOT_VERSION`（msgid 148，含 `uid` / `uid2`），取 `uid` → 经 `IdentityReport` 与 `Telemetry.unique_id` 上报（`Register` 不再携带 `devices` / `identities`，§5.8） |
| 校验 | 注册阶段不校验唯一标识；建链后由 `IdentityReport` 校验（不符即撤销分配），下发指令前再校验一次（用于换机/串机检测） |

判定规则（唯一定义处）：

- 档案 `unique_id` **为空**（未登记）→ 不做唯一标识判定，放行并告警；前端提示应尽快登记。
- 档案 `unique_id` **非空** → 要求上报值非空且相等；上报为空或不相等 → 拒绝（撤销分配 /
  不下发指令）+ 告警。上报为空同样按不一致处理，避免"不提供唯一标识"成为绕过口子。

密钥与暴露面：

- `signing_secret` 在库里按**密文**存（应用层加密，主密钥来自 drone-service 的环境变量或密钥
  管理，不入库）；下发时解密，在已认证的 gRPC 通道上传给 adapter，adapter 不落盘。
- 因此 §12.2 的 mTLS 是**密钥分发的前置**：未开启 mTLS 时，任何能连上 50051 的主机注册同名
  `adapter_id` 就能拿到端点与签名密钥——拿到密钥即可伪造飞控签名，反而绕过 L2。两项必须
  同批上线，不可只做其一。
- 端点本身不是密钥，但会暴露拓扑，同样只经已认证通道下发。
- adapter 本地不持久化端点与密钥：重启后重新注册获取，避免密钥落在机载磁盘上。

边界与口径：

- 格式：小写十六进制字符串。`uid` 的多字节序随固件不同，**以实测上报值为准**，服务端不做字节序推断。
- 边界（必须写进运维文档）：`unique_id` 是飞控**自述**的，能直接连上飞控的人可以伪造它。
  因此 L3 防的是平台内错控、串机与未登记机体占用设备号；**防"直接连飞控下指令"靠的是 L1（+L2）**。
- 与 §12.2 不重叠：§12.2 保护云链路（平台 ↔ 适配器），本节保护机载链路（适配器 ↔ 飞控）。

#### 落地顺序

1. 部署侧：WireGuard 隧道 + 飞控端口只绑隧道内地址（不改代码即生效，收益最大）。
2. 云链路：§12.2 mTLS（客户端证书 + 服务端验签），**与第 4 步同批上线**（否则密钥明文过网）。
3. 数据模型：`drone_device` 加 `unique_id` / `mavlink_addr` / `signing_secret` 三列 + 幂等迁移脚本。
4. 服务端：注册时按 `adapter_id` 查库分配，`RegisterAck` 下发 assignments；建链后按 `IdentityReport` 校验唯一标识，并在指令下发前再校验一次。
5. 适配器：启动顺序倒置（先注册后连飞控）、多机连接池、按 assignment 建链与签名。
6. 前端与运维：无人机档案表单增加唯一标识 / MAVLink 端点 / 签名密钥三项录入；补飞控侧
   人工配置同一把密钥的 SOP，以及"收到帧数 / 验签通过帧数"两项计数与告警（§12.4 L2）。

---

## 13. 部署

### 13.1 MVP（docker-compose）

```yaml
services:
  rustcam:
    image: rustcam-media:latest          # 镜像名沿用 bin 名；容器内跑 rustcam-server 进程
    working_dir: /app
    volumes:
      # 代码固定从工作目录读 config.toml（AppConfig::load 写死 "config.toml"），
      # Casbin 启动时读同目录下的 casbin_model.conf；策略从数据库加载
      - ./config.toml:/app/config.toml:ro
      - ./casbin_model.conf:/app/casbin_model.conf:ro
    ports:
      - "8081:8081"
    depends_on:
      - redis
      - postgres
      - zlmediakit
      - rustcam-drone-service

  rustcam-drone-service:                 # 无人机控制面，独立进程
    image: rustcam-drone-service:latest
    working_dir: /app
    volumes:
      - ./drone.toml:/app/drone.toml:ro
    environment:
      - DRONE_REDIS_URL=redis://redis:6379
      - DRONE_DATABASE_URL=postgres://postgres:123456@postgres:5432/rustcam
    expose:
      - "50051"                          # gRPC，仅容器网络可达
      - "50052"                          # 内网 /metrics
    depends_on:
      - redis
      - postgres

  rustcam-drone-adapter:
    image: rustcam-drone-adapter:latest
    environment:
      - ADAPTER_ID=adapter-mavlink-1
      - PROTOCOL=mavlink
      - SERVICE_ADDR=http://rustcam-drone-service:50051
    depends_on:
      - rustcam-drone-service

  redis:
    image: redis:7
    command: ["redis-server", "--appendonly", "yes"]

  postgres:
    image: postgis/postgis:16-3.4          # 应用启动会执行 CREATE EXTENSION postgis
    environment:
      - POSTGRES_PASSWORD=123456
      - POSTGRES_DB=rustcam

  zlmediakit:                              # 现有视频/媒体服务（无人机视频接入走它）
    image: zlmediakit/zlmediakit:master
    ports:
      - "8080:8080"                        # HTTP API
      - "554:554"                          # RTSP
```

> 容器内的 `config.toml` 需要把 `database.url`、`redis.url` 改成服务名（`postgres:5432`、`redis:6379`），SIP `bind_ip` 改成 `0.0.0.0`，并把 `media_servers[0].url` 指向 `http://zlmediakit:8080`；同时新增 `[drone] service_addr = "http://rustcam-drone-service:50051"`。仓库现有的 `config.toml` 是宿主机配置（pg 5435、SIP bind_ip 固定 IP），直接挂进容器会绑定失败。

三个进程各自独立：`rustcam` 对外暴露 8081；`rustcam-drone-service` 只在容器网络暴露 50051/50052；`rustcam-drone-adapter` 不暴露任何端口。

### 13.2 生产

```text
rustcam-server       x N（信令/媒体/API/WS，/api/v1/drone/* 唯一对外入口）
rustcam-drone-service x N（控制面：只暴露内网 gRPC + Streams 消费组，无对外 HTTP）
rustcam-drone-adapter x N（按协议/设备分片，Redis 抢占）
zlmediakit           x N（现有媒体服务，视频流接入）
redis                主从/哨兵
postgres/postgis     主从
timescaledb          可选，独立实例或同库扩展
边缘                伴飞计算机跑 edge-adapter，bidi 连中心 service
```

边缘 adapter 要求：

- 只出站，不开放入站。
- 本地环形缓冲，断网补发。
- 证书自动轮换。
- 设备分片：同一设备同一时刻只允许一个 adapter 持有（Redis `SET NX`）。
- MAVLink 链路（§12.4）：飞控端口只绑隧道内地址，禁止 `0.0.0.0`；端点与签名密钥由平台
  统一录入 `drone_device` 后下发（一机一密钥），adapter 本地不持久化。

### 13.3 端口规划

| 端口 | 用途 | 暴露范围 |
|---|---|---|
| 8081 | rustcam HTTP/WS | 内网/前端 |
| 5060/5061 | SIP TCP/UDP | 现有规划 |
| 5541 | RTSP | 现有规划 |
| 50051 | drone-service gRPC | 仅内网/容器网络 |
| 50052 | drone-service 内网 /metrics | 仅内网 |
| 15000-15100 | RTP 端口段（`signaling_server.rtp_port_start/end`） | 内网 |
| 8080/554/1935/8000/10000 | ZLMediaKit（HTTP/RTSP/RTMP/WS-FLV/RTC） | 内网 |
| 6379 | Redis | 仅内网 |
| 5435 | PostgreSQL（宿主机；容器内为 5432） | 仅内网 |
| 10.88.0.0/24 | 机载 WireGuard 隧道网段（MAVLink 跑在隧道内，见 §12.4） | 仅隧道内 |

---

## 14. 可观测性与测试

### 14.1 指标

```text
rustcam_drone_adapter_connections_total{adapter_id}
rustcam_drone_adapter_reconnect_total{adapter_id}
rustcam_drone_adapter_heartbeat_age_seconds{adapter_id}
rustcam_drone_command_duration_seconds{type,status}
rustcam_drone_command_inflight
rustcam_drone_command_timeout_total{type}
rustcam_drone_telemetry_lag_seconds{drone_tag}
rustcam_drone_telemetry_dedup_dropped_total
rustcam_drone_stream_pending{stream,group}
rustcam_drone_stream_length{stream}
rustcam_drone_geofence_eval_duration_seconds
rustcam_drone_geofence_alert_total{level}
rustcam_drone_control_acquire_total{result}
rustcam_drone_control_preempt_total
rustcam_drone_ws_subscribers{channel}
rustcam_drone_ws_lagged_total{channel}
```

不引入独立 Prometheus registry：主服务 `monitoring::Metrics` 已是手写 Prometheus 文本输出（现有指标统一带 `rustcam_` 前缀）。drone 指标由 drone-service 自己的内网 `/metrics`（50052）暴露，同样是手写文本、同样带 `rustcam_drone_` 前缀；文本格式化逻辑放在 `rustcam-drone-core::metrics`，供 drone 侧复用。Grafana/Loki 属于部署侧，可后置。

指标名与标签在 `rustcam-drone-core::metrics::names` 中集中定义，业务代码只引用常量。补充登记的指标（实现已产出，一并在此登记以免两处对不上）：

```text
rustcam_drone_telemetry_total
rustcam_drone_telemetry_stale_total
rustcam_drone_telemetry_rejected_total
rustcam_drone_command_sent_total{type}
rustcam_drone_command_reply_total{type,status}
rustcam_drone_risk_alert_total{rule}
rustcam_drone_online_devices
rustcam_drone_adapters
rustcam_drone_fences
```

约定：

- `rustcam_drone_command_duration_seconds{type,status}` 当前取**最近一次**指令耗时（gauge）；需要 P99 时再按 §14.2 补固定桶直方图。
- `rustcam_drone_ws_subscribers{channel}` / `rustcam_drone_ws_lagged_total{channel}` 的 `channel` 在 drone-service 侧为 `event`（服务内部只有一条事件总线）；浏览器侧按路径拆分的通道由主服务统计。
- 事件订阅者落后（broadcast `Lagged`）时**丢弃旧消息并继续**，同时计数 `rustcam_drone_ws_lagged_total`；不得因此终止订阅。
- `rustcam_drone_stream_pending` / `rustcam_drone_stream_length` 依赖 Redis Streams，随 §8 的 Streams 落地后补齐。

两个进程的指标分开抓取：

| 端点 | 进程 | 指标 |
|---|---|---|
| `/api/v1/metrics` | rustcam-server | 现有 `rustcam_*` |
| `http://<drone-service>:50052/metrics` | rustcam-drone-service | `rustcam_drone_*` |

标签基数说明：`drone_tag` 维度只在必要时用（如 `telemetry_lag_seconds`），高基数场景按 adapter 聚合，避免 Prometheus 基数爆炸。

### 14.2 告警

- `XPENDING` 持续增长。
- 指令平均耗时/超时率超阈值（当前指标是扁平 counter/gauge，算不了 P99；需要 P99 时加固定桶直方图 `_bucket/_sum/_count`）。
- adapter 掉线或心跳年龄 > 3 个周期。
- 围栏判断平均耗时超阈值（同上，P99 需要直方图）。
- Redis 内存使用率 > 80%：由部署侧 `redis_exporter` 采集，不在应用指标内。
- gRPC 错误率 > 1%。

### 14.3 日志与追踪

- 统一 `tracing`，字段包含 `drone_tag`、`adapter_id`、`cmd_id`、`operator`。
- gRPC/HTTP 入口生成 `trace_id`，透传到 adapter。
- 审计日志与业务日志分离，审计写库。

### 14.4 测试分层

| 层级 | 内容 |
|---|---|
| 单测 | 围栏 geo 判断、Lua 控制权、指令幂等、优先级、参数校验 |
| 集成 | adapter ↔ service bidi、Redis Streams 消费、控制权锁、影子更新 |
| 鉴权 | 无 token 调 `/api/v1/drone/*` 返回 401；viewer 角色被拒；ws-ticket 过期/重放被拒 |
| E2E | docker compose + PX4 SITL headless + 脚本 |
| 故障 | 丢包/延迟（tc netem）、kill adapter、停 Redis、重复注册、慢 WS |
| 性能 | 多 SITL 实例压 adapter/service，观察 P99、内存、Streams 堆积 |

### 14.5 关键测试用例

- 无持有者时 acquire 成功。
- 租约过期后 acquire 成功。
- 有人持有且未过期时 acquire 失败（不抢占）。
- 同优先级 preempt 失败；更高优先级 preempt 成功并返回旧 holder。
- 非 holder release 失败。
- 非 holder renew 失败。
- 重复 `cmd_id` 只执行一次。
- `deadline_ms` 到期标 timeout。
- 禁飞区 `goto` 被拒绝。
- 进入 `no_fly` 触发 high 告警；连续 2 个采样才触发、离开 3s 恢复。
- adapter 断线后设备离线、控制权到期。
- 重复 adapter 注册旧连接被踢。
- 无 token 访问 drone REST 返回 401。
- ws-ticket 过期后连接被拒，ticket 不可重放。
- Redis 不可用时启动不失败，控制权降级为内存锁并告警。

---

## 15. 实施路线图

### 阶段 0：workspace 与契约（1-2 天）

- 根 Cargo.toml 改 workspace。
- 迁移原 lib/bin，保持 bin 名 `rustcam-media`；同时建 `crates/rustcam-server`（bin）、`crates/rustcam-drone-service`（bin）、`crates/rustcam-drone-adapter` 空壳。
- 新建 `rustcam-drone-proto`（proto 编译通过）与 `rustcam-drone-core`（纯类型/常量）。
- 验收：`cargo build --bin rustcam-media` 成功，现有功能不变。

### 阶段 1：drone-service 骨架（2-3 天）

- `rustcam-drone-service`：自有 config（`drone.toml`）、`infra/redis.rs`、`infra/db.rs`（执行 `migrations.sql` 建 drone_* 表）、`state.rs`。
- gRPC server :50051（AdapterLink + Control/Query/Geofence/Event）；启动时校验端口未占用。
- 自建 `DroneRedis`（Lua/Streams/SET NX）与 `DroneDb`。
- 验收：服务能独立启动，表建好，gRPC 可连。

### 阶段 2：主服务网关（2-3 天）

- `rustcam-media` 增加 `DroneClient`（tonic client 封装四个 service）与 `AppState.drone`。
- 主服务加 `[drone] service_addr` 配置。
- `GET /api/v1/drone/ping`、`GET /api/v1/drone/devices`（内部转 gRPC）。
- drone 路由并入 `protected_routes`。
- 验收：无 token 访问 drone 接口返回 401；有 token 能拿到设备列表。

### 阶段 3：遥测闭环（2-3 天）

- 假 adapter 定时发遥测；drone-service 去重、影子、EventService 广播。
- 主服务订阅 EventService → `/ws/drone/telemetry` 推送；`POST /api/v1/drone/ws-ticket` + `?ticket=` 鉴权。
- WS 消息包沿用现有 `{msg_type, data, timestamp}`。
- 前端 `frontend/src/views/drone/Map2D.vue` 显示假飞机（可复用 `views/DeviceMap.vue` 的 MapLibre + terra-draw 初始化逻辑）。
- 验收：浏览器 WS 收到遥测，地图位置更新；无 ticket / ticket 过期连接被拒。

### 阶段 4：真实 MAVLink（3-5 天）

- `rustcam-drone-adapter` 接 `rust-mavlink`，连 PX4 SITL。
- 解析姿态/位置/电量。
- 验收：SITL 遥测驱动前端模型。

### 阶段 5：分控（2-3 天）

- drone-service：Lua acquire/release/renew/preempt/force_release（`EVALSHA` + `NOSCRIPT` 重载）+ 审计。
- 主服务：对应 REST + Casbin 显式调用 `CasbinManager::enforce`，并在 `permissions`/`role_permissions` 中补 drone 策略。
- 验收：两个客户端抢权互斥，抢占返回旧 holder；viewer 角色调 `control:acquire` 返回 403。

### 阶段 6：指令（3-5 天）

- drone-service：指令状态机、幂等、超时、路由。
- adapter 执行 RTL/land/takeoff。
- 回执与 WS。
- 验收：RTL 下发后状态从 acked → executed，超时场景正确。

### 阶段 7：围栏与风控（3-5 天）

- drone-service：围栏 CRUD、缓存、geo 判断、越界/低电量/失联告警。
- 主服务：围栏 REST（GeofenceService）。
- terra-draw 编辑、Cesium 限高展示。
- 验收：禁飞区越界触发 high 告警，WS 可见。

### 阶段 8：生产化（5-10 天）

- mTLS（rustcam-server↔drone-service、drone-service↔adapter）、审计查询、指标、告警。
- 故障注入、性能压测、多实例与分片。
- 验收：故障矩阵通过，指标可观测，部署文档完备。

---

## 16. 风险与注意事项

| 风险 | 影响 | 处理 |
|---|---|---|
| drone 路由挂在 `create_router` 之后 | 绕过 JWT，所有 drone 接口裸奔 | 必须在 `protected_routes` 内 merge，或显式加 `jwt_auth_layer` |
| `RedisBus` 无 Lua/Streams | 控制权与异步事件无法实现 | drone-service 自建 `DroneRedis`（按 8.3 实现 `script_load/eval_sha/xadd/xreadgroup/...`），不复用主服务 `RedisBus` |
| 浏览器 WS 无法带 Authorization | drone WS 无鉴权可看 | `ws-ticket` 一次性票据 + 升级前校验 |
| drone 表无人创建 | 表不存在，功能直接报错 | drone-service 启动执行自带 `migrations.sql`（幂等） |
| DB 权限策略未配置、Casbin 未接入路由 | 越权操作 | drone handler 显式 `enforce`，并在 `permissions`/`role_permissions` 补策略 |
| `registry.infra.db/redis` 类型不匹配 | `DroneState` 初始化失败 | drone-service 独立进程后不再复用主服务 infra：自建 `DroneRedis` / `DroneDb` |
| drone-service 与主服务版本不一致 | gRPC 调用失败 | proto 只增不改；主服务与 drone-service 独立发版，gRPC 错误映射为 5xx |
| `pub(crate)` 跨 crate 不可见 | workspace 拆分后编译失败 | 逐步将 main.rs 用到的项改 `pub`；避免一次性大拆 |
| tonic 编译变慢 | 开发体验下降 | proto 精简、增量编译；CI 缓存 |
| `protoc` 缺失 | `rustcam-drone-proto` 构建失败 | 使用 `protoc-bin-vendored` 或 CI 安装 |
| rbatis 对 PostGIS 支持弱 | 围栏存储受阻 | MVP 用 `geojson text` + `geo`，不碰 geometry |
| Cesium 包体积大 | 前端首屏变慢 | 路由懒加载、独立 chunk、CDN 静态资源 |
| terra-draw 与 MapLibre 版本兼容 | 围栏绘制异常 | 锁定现有版本，升级前做回归 |
| gRPC/HTTP/SIP 端口冲突 | 启动失败 | 统一端口规划表，启动时校验 |
| `tokio::spawn` panic | 拖垮主进程 | 每个长任务捕获错误并记录；必要时 `catch_unwind` |
| Redis 单点 | 控制权不可用 | 生产主从/哨兵；降级为单实例内存锁并告警 |
| 重复 adapter 注册 | 设备双写/指令重复 | 新连接胜出 + generation 校验 |
| 指令重复下发 | 飞控重复执行 | `cmd_id` 幂等 + adapter 侧去重 |
| 前端消息未校验 | 页面崩溃 | 所有 WS/REST 消息运行时校验 |
| 审计表增长过快 | 查询变慢 | 按月分区或定期归档 |
| 时区/单位不一致 | 轨迹/围栏判断错误 | 统一 UTC ms、度、米、弧度 |

---

## 17. 附录

### 17.1 命令与状态码

| 状态 | 含义 |
|---|---|
| `pending` | 已接收，未路由 |
| `sent` | 已下发 adapter |
| `acked` | adapter 已确认 |
| `executed` | 飞控执行完成 |
| `failed` | 执行失败 |
| `timeout` | 超时 |
| `rejected` | 校验失败/无权限/无控制权 |

### 17.2 控制权动作

| 动作 | 说明 |
|---|---|
| `acquire` | 申请；仅在无人持有或已过期时成功，不抢占 |
| `renew` | 续约 |
| `release` | 主动释放（仅 holder） |
| `preempt` | 高优先级强制抢占 |
| `expire` | TTL 到期自动释放（由 drone-service reaper 广播） |
| `force_release` | 管理员强制释放 |

### 17.3 告警级别

| 级别 | 含义 |
|---|---|
| `low` | 提示 |
| `medium` | 需要关注 |
| `high` | 需要立即处置 |
| `critical` | 飞行安全事件 |

### 17.4 术语

| 术语 | 含义 |
|---|---|
| adapter | `rustcam-drone-adapter`，协议适配进程，负责 MAVLink/厂商 SDK |
| service | `rustcam-drone-service`，控制面进程，负责分控/指令/遥测/围栏/风控 |
| gateway | `rustcam-server`，drone 的唯一对外 HTTP/WS 入口与鉴权点 |
| 设备影子 | Redis 中每台设备的最新状态 |
| 租约 | 控制权的有效期，到期自动释放 |
| 抢占 | 高优先级操作者强制获取控制权 |
| 围栏 | 禁飞区/限高区/工作区/返航区 |

### 17.5 与现有系统的接入点

| 现有能力 | 复用方式 |
|---|---|
| JWT | drone REST 鉴权（复用 `jwt_auth_layer`，在 rustcam-server 侧）；WS 用 `ws-ticket` |
| Casbin | `drone` 资源与动作策略来自 `permissions`/`role_permissions`（需显式接入 enforce，在 rustcam-server 侧） |
| axum router | 在 `protected_routes` 内合并 `/api/v1/drone/*`；`/ws/drone/*` 单独注册 |
| Redis | drone-service 自建 `DroneRedis`：控制权、注册表、影子、Streams |
| rbatis/PostgreSQL | drone-service 自建 `DroneDb`，只操作 `drone_*` 表：审计、设备、围栏 |
| ONVIF/RTSP/ZLMediaKit | 无人机视频流接入现有播放链路（主服务侧，不经 drone-service） |
| 现有 `EventBus` | 不共享；drone-service 用自己的本地总线，跨进程走 Redis Streams/gRPC |
| 现有 `Metrics` | 不共享；drone-service 自暴露内网 `/metrics`，格式与现有 Prometheus 文本一致 |
| 现有 `ServiceRegistry` | 不共享；drone-service 独立读自己的 `drone.toml` |

---

## 18. 最终形态

```text
RustCam-Media（workspace）
├── crates/rustcam-media      现有 SIP/媒体/ONVIF/RTSP + drone 对外网关（REST/WS）
├── crates/rustcam-server     组装进程，bin 名仍为 rustcam-media
├── crates/rustcam-drone-proto    gRPC 契约（common/adapter_link/control）
├── crates/rustcam-drone-core     纯类型/常量/metrics 文本（service + adapter 共享）
├── crates/rustcam-drone-service  无人机控制面独立进程（含 migrations.sql、自有 infra）
├── crates/rustcam-drone-adapter  MAVLink/厂商协议适配独立进程
├── frontend                  Vue 3 + MapLibre + terra-draw + Cesium
└── sql/schema.sql            主服务表结构（drone_* 表由 drone-service 自带迁移）

共享边界：只共享 rustcam-drone-proto + rustcam-drone-core；不抽 EventBus，不建 rustcam-infra。

命名对照：crate 名 rustcam-server / bin 名 rustcam-media / 镜像名 rustcam-media，
指同一个进程；容器 service 名可任意（compose 里叫 rustcam）。

运行形态（MVP 与生产同构，三个进程）：
  rustcam-server x N        唯一对外 HTTP/WS 入口（8081，/api/v1/drone/*）
        ▲ gRPC(ControlService / QueryService / GeofenceService)   ▲ gRPC(EventService, 事件回流)
  rustcam-drone-service x N 只暴露内网 gRPC（50051）+ 内网 /metrics（50052）
        ▲ gRPC(AdapterLink, bidi)
  rustcam-drone-adapter x N 按设备分片，不对外暴露端口
        │ MAVLink
  PX4 SITL / ArduPilot / 真机
  Redis 主从 / PostgreSQL 主从 / TimescaleDB
```

**设计闭环：** 契约先行（rustcam-drone-proto），控制面与协议面分离，同步走 gRPC、异步走 Streams，控制权走 Redis Lua，围栏先 GeoJSON 后 PostGIS，前端复用现有 Vue 工程，按阶段增量落地，每步可编译、可回滚、可验证。
