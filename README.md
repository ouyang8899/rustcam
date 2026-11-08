# RUSTCAM

**RUSTCAM** —— 基于 **Rust + Vue** 的现代化视频监控网关与信令控制平台。

支持 **RTSP / GB28181 / ONVIF** 等多协议设备接入，提供**实时视频播放、录像回放、云台 PTZ 控制、设备分组管理**等功能；前端采用 **Vue 3 + Element Plus**，兼顾性能与安全。同时提供 Web 控制台与 Qt 桌面客户端，可私有化部署在客户自有环境中。

在固定点位之外，平台正把监控域扩展到**会动的视点**——**移动眼**：无人机等移动视频源接入同一套录像、告警与证据链，让"需要时再飞过去看"成为监控体系的常规能力（建设中，详见下文）。

## 仓库地址

| 平台 | 地址 |
|------|------|
| Gitee | https://gitee.com/ouliuying/rustcam |
| GitHub | https://github.com/ouyang8899/rustcam |

- 官网：https://www.rustcam.com
- 联系邮箱：ouyang.8899@hotmail.com

## 功能特性

| 类别 | 功能 |
|------|------|
| 视频服务 | 实时播放、多画面布局、录像回放 |
| 设备管理 | 设备发现、注册、状态监控、分组管理 |
| 云台控制 | PTZ 协议支持、预置位管理 |
| 流媒体 | 集群部署、多服务器适配 (ZLMediKit/SRS/Xiu) |
| 电子地图 | MapLibre GL JS 地图、设备标记聚合、状态筛选、设备加载 |
| 用户权限 | JWT 认证、RBAC 权限控制（角色 / 权限 / 角色继承，策略存数据库） |
| 移动眼 | 无人机等移动视频源接入，与固定摄像机共用同一套录像、告警与证据链（建设中） |

## 技术栈

### 后端 (Rust)

| 层级 | 技术 |
|------|------|
| 协议 | RTSP、GB28181、ONVIF、WebSocket |
| 认证 | JWT + Casbin |
| 存储 | Redis (缓存) + PostgreSQL/PostGIS (业务与档案) + TimescaleDB (遥测时序) |

### 前端 (Vue)

| 类别 | 技术 |
|------|------|
| 框架 | Vue 3 + Element Plus |
| 状态 | Pina |
| 图表 | ECharts + Vue-ECharts |
| 视频 | flv.js、hls.js、video.js |
| 构建 | Vite |

### 桌面版 (Qt/C++)

| 类别 | 技术 |
|------|------|
| 框架 | Qt 5/6 |
| 构建 | CMake |
| 音视频 | FFmpeg |

## 系统架构

后端采用 **Axum + Tokio** 分层架构：HTTP 处理器 → 应用服务 → 领域模型/端口 → 基础设施实现；协议侧通过 `SignalAdapter` 统一抽象 GB28181 / ONVIF / RTSP / WebSocket；传输与媒体数据面（`src/transport` + `src/media`）负责 SIP 信令与 RTP 收发，并通过 **MediaNode** 中间件链完成 G711 ⇄ PCM 转换与重采样，支撑 GB28181 广播 / ONVIF Backchannel 语音对讲；媒体侧通过 `MediaServerAdapter` 对接 ZLMediaKit / SRS / XIU。

![RustCam-Media 系统架构](./public/architecture.png)

> 架构图源码为 [`public/architecture.mmd`](./public/architecture.mmd)（Mermaid），修改后执行 `node public/render-architecture.mjs` 可重新生成 PNG。

## 移动眼（Mobile Eye）

> **固定摄像机看住该看的，移动眼盯住会变的。**

RustCam 正在把监控域从**固定点位**扩展到**会动的视点**——我们把它叫做**移动眼**：无人机等移动视频源可以作为一只"移动眼"接入同一套平台，它们的视频、位置、告警与固定摄像机共用同一套录像、证据链、权限和监控墙。

移动眼不是"会飞的摄像头"：无人机自带**飞控**（PX4 / ArduPilot 等），它决定去哪看、怎么看、何时回。平台要接的是这一整套飞行能力，而不只是把相机搬上天——飞控协议通过适配器接入，不写死在平台里，换机型、换厂商不换平台。

### 两条原则

| | 说明 |
|---|---|
| **不新增固定点位** | 需要时才调度：省掉布线、杆件、点位与后续维护，视觉覆盖按需扩展而不是提前埋满 |
| **不锁场景** | 航线、围栏、告警规则以配置组合而成；巡检 / 安防补盲 / 应急取证是同一套能力的不同模板，不是不同产品 |

### 组成

| 组件 | 职责 |
|---|---|
| `rustcam-drone-service` | 独立控制面进程：设备注册表、遥测去重与影子、控制权仲裁、指令状态机、围栏风控、事件广播 |
| `rustcam-drone-adapter` | 机载适配进程：对接**飞控**（MAVLink / PX4 / ArduPilot / 厂商 SDK），把无人机同时接入"视频源"与"移动设备"两个维度 |
| `rustcam-server` | 唯一对外入口：JWT + RBAC 鉴权、`/api/v1/drone/*` REST 网关、`/ws/drone/*` 实时推送 |
| GB28181 / RTSP 接入 | 机载摄像头以标准协议接入，视频、录像、回放、监控墙零改动复用 |

### 能力

| 能力 | 说明 |
|---|---|
| 飞控接入 | MAVLink（PX4 / ArduPilot）与厂商 SDK 经适配器接入，飞控差异被隔离在适配层 |
| 飞行指令 | 起飞、降落、悬停、航点、返航等指令直达飞控，带状态机、回执与审计；失控保护与自动返航由飞控自身兜底 |
| 云台控制 | 相机云台与飞控解耦，可独立控制朝向、变焦与拍照 |
| 位置与遥测 | 位置、姿态、电池、链路状态实时上云，地图与轨迹随动 |
| 控制权仲裁 | 飞行 / 云台控制权带租约与抢占，多人、多系统同时操作不打架 |
| 围栏风控 | 禁飞区、限高区自动判定，触发即产生告警事件并进入统一告警流 |
| 合规内置 | 飞行日志与操作审计随平台落库（`drone_control_audit` / `drone_command_audit`）；禁拍区、作业时段为规划项 |

### 现状与规划

| 阶段 | 状态 |
|---|---|
| 三进程骨架（控制面 / 适配 / 网关）、设备注册与心跳、REST 网关、DB 驱动 RBAC | ✅ 完成 |
| 真实 MAVLink 飞控接入（PX4 / ArduPilot）、遥测去重与指令回执 | ✅ 完成 |
| 移动眼前端：无人机档案、Cesium 3D 实时飞行、航线规划、电子围栏、告警审计、轨迹回放 | ✅ 完成 |
| 控制权仲裁 / 围栏风控 / 指令状态机 / 审计留痕 | ✅ 完成（Redis Lua 租约跨实例互斥，Redis 不可用时降级为进程内锁） |
| 模板化场景、移动眼与监控墙联动演示 | 🔄 进行中 |

### 快速接入（PX4 / ArduPilot）

```bash
# 一键测试环境（无需真实飞控）：模拟器 + adapter + drone-service + rustcam-server + 前端
pwsh -File testenv\start.ps1
```

手动启动：

```bash
cp adapter.toml.example adapter.toml
# adapter.toml 只保留进程级配置：adapter_id / service_addr / autopilot。
# 设备清单、MAVLink 端点、签名密钥与 sysid 由平台档案分配下发，本地不再配置。

cargo run -p rustcam-drone-service
cargo run -p rustcam-drone-sim          # 无飞控时使用内置 MAVLink 模拟器
cargo run -p rustcam-drone-adapter
```

> 标识模型：`drone_tag` 是平台主键（建档时自动生成 `drone-<uuid4>`，REST/前端/Redis/审计统一
> 使用）；`unique_id` 是飞控自述的物理标识；`(adapter_id, mavlink_sysid)` 是链路绑定。
> 首次接入请在「移动眼 → 无人机」新增档案，填写唯一标识、`adapter_id`、MAVLink 端点与 sysid；
> adapter 注册后由 drone-service 按 `adapter_id` 分配，不再使用本地设备 ID。

适配器按 `HEARTBEAT.autopilot` 自动识别 PX4 / ArduPilot，并把两侧的模式编码统一成
`AUTO.LOITER` / `GUIDED` 等名称；遥测经 gRPC 上行到 drone-service 并做去重与影子缓存，
指令经 `ControlService.SendCommand` 下行，由 `COMMAND_ACK` 回执闭环。

#### 唯一标识核验

平台按「档案登记的唯一标识 == 飞控实测的唯一标识」判定是否放行指令（[drone.md](./drone.md) §12.4 L3）。
模拟器通过 `AUTOPILOT_VERSION.uid` 上报实测值（十六进制，≤16 位）：

```bash
# 指定模拟器的唯一标识（默认 0011223344556677，也可用环境变量 DRONE_SIM_UID）
cargo run -p rustcam-drone-sim -- --uid 0011223344556677
```

在「无人机」档案的**唯一标识**字段填入同一个值（小写十六进制）即可正常下发指令；
填成别的值时，指令会被判 `unique_id mismatch` 拒绝，该机的分配也会被撤销并留告警。
档案留空则不做判定（放行）。

> 实机边界：MAVLink 2 的 `AUTOPILOT_VERSION.uid2`（18 字节）不在 ardupilotmega v1.0 方言内，
> 真实 PX4 / ArduPilot 把 UUID 放在 uid2，因此实机当前读不到唯一标识；模拟器用 `uid`（u64）承载，
> 与平台登记值一一对应。

前端「移动眼」菜单包含：

| 页面 | 能力 |
|---|---|
| 无人机 | 档案增删改查、机型/飞控/视频通道绑定 |
| 控制权管理 | 飞行 / 云台控制权申请、抢占、续约与释放，租约时长与持有人可见 |
| 实时飞行 | Cesium 3D 地球、实时遥测、控制权申请/释放、起飞/降落/返航/定点/云台/拍照、指令回执 |
| 航线规划 | 地球点击添加航点、排序/高度/速度/悬停、保存与执行 |
| 电子围栏 | 多边形/圆形、禁飞区/允许区、高度区间、越界告警、goto 预检 |
| 告警与审计 | 围栏越界、上下线、指令与控制权操作留痕 |
| 轨迹回放 | 按时间段查询历史轨迹，2D/3D 视图切换与播放进度 |

> Cesium 底图默认使用离线 NaturalEarth，内网/演示环境不依赖外部瓦片；
> 生产可在前端构建时设置 `VITE_CESIUM_TMS=<内网瓦片模板>`，或 `VITE_CESIUM_ONLINE=true` 使用在线底图。
> 无外网时离线底图分辨率有限，高清影像接入由后续配置完成。
>
> 当前限制：控制权仲裁走 Redis Lua 脚本（跨实例互斥；Redis 不可用时降级为进程内锁，只保证单实例互斥）；
> 档案、航线、围栏、审计已落 PostgreSQL（`drone_*` 表由 drone-service 启动时自建），
> `drone-state.json` 仅作快照兜底与旧数据回填；drone gRPC 尚未启用 mTLS/token。

## 截图预览

### 前端控制台 (Vue 3)

**系统概览**

![系统概览](./screenshot/2026-10-29/系统概览.png)

**设备列表**

![设备列表](./screenshot/2026-10-29/设备管理-设备列表.png)

**通道列表**

![通道列表](./screenshot/2026-10-29/设备管理-通道列表.png)

**电子地图 - 视频查看**

![电子地图 - 视频查看](./screenshot/2026-10-29/设备列表-电子地图-视频查看.png)

**电子地图 - 聚合点位**

![电子地图 - 聚合点位](./screenshot/2026-10-29/设备列表-电子地图1.png)

**电子地图 - 设备点位**

![电子地图 - 设备点位](./screenshot/2026-10-29/设备列表-电子地图2.png)

**流列表**

![流列表](./screenshot/2026-10-29/流列表-流列表.png)

**实时会话**

![实时会话](./screenshot/2026-10-29/流列表-实时会话.png)

**录像管理**

![录像管理](./screenshot/2026-10-29/录像管理-录像管理.png)

**录像文件**

![录像文件](./screenshot/2026-10-29/录像管理-录像文件.png)

**录像策略**

![录像策略](./screenshot/2026-10-29/录像管理-录像策略.png)

**录像回放**

![录像回放](./screenshot/2026-10-29/录像管理-录像回放.png)

**媒体服务器管理**

![媒体服务器管理](./screenshot/2026-10-29/媒体服务器.png)

**报警记录**

![报警记录](./screenshot/2026-10-29/报警记录.png)

**监控墙**

![监控墙](./screenshot/2026-10-29/监控墙-监控墙.png)

**监控墙布局设计**

![监控墙布局设计](./screenshot/2026-10-29/监控墙-布局设计.png)

**系统设置**

![系统设置](./screenshot/2026-10-29/系统设置-系统设置.png)

### 桌面客户端 (Qt)

**登录**

![登录](./screenshot/2026-10-29-destop/客户端-登录.png)

**实时预览**

![实时预览](./screenshot/2026-10-29-destop/客户端-实时预览.png)

**电子地图**

![电子地图](./screenshot/2026-10-29-destop/客户端-电子地图.png)

**电子地图 - 视频预览**

![电子地图 - 视频预览](./screenshot/2026-10-29-destop/客户端-电子地图-视频预览.png)

**录像回放**

![录像回放](./screenshot/2026-10-29-destop/客户端-录像回放.png)

**报警记录**

![报警记录](./screenshot/2026-10-29-destop/客户端-报警记录.png)

**系统设置**

![系统设置](./screenshot/2026-10-29-destop/客户端-系统设置.png)

## API 说明

后端基于 Axum 暴露 HTTP + WebSocket 接口，业务接口统一前缀 `/api/v1`，共 **102 条受保护路由** 与 **14 条公开路由**。完整接口清单（按业务域分组、含请求体与响应字段）见 **[`public/api.md`](./public/api.md)**。

### 认证

除公开端点外，所有接口都需在请求头携带 JWT：

```http
Authorization: Bearer <access_token>
```

```bash
# 1) 登录获取令牌
curl -s -X POST http://127.0.0.1:8081/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin"}'

# 2) 携带令牌访问业务接口
curl -s "http://127.0.0.1:8081/api/v1/devices?limit=20" \
  -H "Authorization: Bearer <access_token>"
```

### 统一响应格式

```json
{ "code": 200, "message": "success", "data": { } }
```

失败时 `code` 为 400 / 401 / 403 / 404 / 500 / 502 / 503，`message` 为中文错误描述，`data` 为 `null`。

> ⚠️ 两个例外：`/hook/*` 与 `/alarm/{stream_key}` 返回 `{"code": 0, "msg": "ok"}`（`code` 为 `0`，字段名是 `msg`）；`GET /api/v1/stats` 返回不带封装的裸 JSON。

### 接口分组速览

**受保护路由（102 条，需携带 JWT）**

| 分组 | 端点数 | 主要端点 |
|------|:------:|----------|
| 用户 / 角色 / 权限 | 11 | `GET/POST /users`、`PUT /users/{id}/roles`、`GET/POST /roles`、`PUT /roles/{id}/permissions`、`GET/POST /permissions` |
| 设备 | 13 | `GET/POST /devices`、`POST /devices/{id}/play`、`GET /devices/{id}/config` |
| 通道 | 7 | `GET/POST /channels`、`GET /channels/{device_tag}/{channel_tag}/status` |
| 云台 PTZ | 6 | `POST /channels/{d}/{c}/ptz`、`GET/POST/PUT/DELETE /channels/{d}/{c}/ptz/presets` |
| 流 | 9 | `GET/POST /streams`、`GET /streams/{id}/play`、`POST /streams/{id}/restart` |
| 会话 | 1 | `GET /sessions` |
| 录像任务 | 10 | `GET/POST /recordings`、`POST /recordings/{id}/start\|stop\|pause\|resume` |
| 录像策略 | 5 | `GET/POST /recording-policies`、`PUT/DELETE /recording-policies/{id}` |
| 录像文件 | 2 | `GET /recordings/files`、`GET /recordings/files/{id}/play-url` |
| 报警 | 2 | `GET /alarms`、`PUT /alarms/{id}/processed` |
| 媒体服务器 | 10 | `GET/POST /servers`、`GET /servers/{tag}/status`、`POST /servers/{tag}/enable` |
| 监控墙布局 | 6 | `GET/POST /layouts`、`PUT /layouts/{id}/default` |
| 设备分组 | 6 | `GET /groups/tree`、`POST /groups`、`PUT /devices/{id}/group` |
| 行政区划 | 2 | `GET /regions`、`GET /regions/tree` |
| ONVIF | 6 | `POST /onvif/discover`、`POST /onvif/probe`、`POST /onvif/devices` |
| 地图设置 | 2 | `GET/PUT /settings/map` |
| 仪表盘 / 缓存管理 | 2 | `GET /dashboard`、`POST /admin/reload-cache` |
| GB28181 参考数据 | 1 | `GET /gb28181/ref-data` |
| 当前用户 | 1 | `GET /auth/me` |

**公开路由（14 条，无需认证）**

| 分组 | 端点数 | 路径 |
|------|:------:|------|
| 登录 / 刷新令牌 | 2 | `POST /api/v1/auth/login`、`POST /api/v1/auth/refresh` |
| 健康 / 指标 / 统计 / 根路径 | 4 | `GET /`、`/api/v1/health`、`/api/v1/metrics`、`/api/v1/stats` |
| 公开直播 | 1 | `GET /api/v1/public/streams` |
| 地图 WFS 代理 | 1 | `GET /api/v1/proxy/wfs` |
| 媒体服务器 Hook | 3 | `/hook`、`/hook/{event_name}`、`/hook/{server_type}/{event_name}` |
| 外部告警上报 | 1 | `POST /alarm/{stream_key}` |
| WebSocket | 2 | `/ws`（事件推送）、`/ws/audio-talk/{device_tag}/{channel_id}`（语音对讲） |

## 模块进度

| 模块 | 功能 | 状态 | 说明 |
|------|------|------|------|
| **GB28181** | SIP 信令 | ✅ 完成 | 支持 UDP/TCP 模式注册、INVITE、BYE |
| **GB28181** | 流接收 | ✅ 完成 | PS 封装解析，RTP over UDP/TCP |
| **GB28181** | 云台控制 | ✅ 完成 | 设备离线时返回 503，适配器自动注册 |
| **GB28181** | 语音对讲 | ✅ 完成 | 平台广播通知|
| **ONVIF** | 设备发现 | ✅ 完成 | WS-Discovery 广播探测 |
| **ONVIF** | 云台控制 | ✅ 完成 | ContinuousMove/AbsoluteMove/预置位 |
| **ONVIF** | 语音对讲 | ✅ 完成 | 双向语音通信|
| **RTSP** | 流接收 | ✅ 完成 | TCP/UDP 模式，支持 RTP over RTSP |
| **流媒体** | ZLMediaKit | ✅ 完成 | 集群管理、Hook 事件处理 |
| **流媒体** | SRS | 🔄 进行中 | 部分实现 |
| **流媒体** | XIU | 🔄 进行中 | 部分实现 |
| **前端** | 实时视频 | ✅ 完成 | flv.js/hls.js 多协议支持 |
| **前端** | PTZ 控制 | ✅ 完成 | D-pad + 摇杆 + 预置位独立卡片 |
| **前端** | 摄像头墙 | ✅ 完成 | 网格布局、拖拽排序 |
| **前端** | 录像回放 | ✅ 完成 | 时间轴检索、回放控制 |
| **前端** | 电子地图 | ✅ 完成 | MapLibre GL JS + GeoServer |
| **认证** | JWT + RBAC | ✅ 完成 | 角色权限动态分配 |
| **认证** | 角色继承 | ✅ 完成 | 策略存数据库，支持角色继承与环检测 |
| **移动眼** | 三进程骨架 | ✅ 完成 | drone-service / drone-adapter / 网关 |
| **移动眼** | 设备注册与心跳 | ✅ 完成 | adapter 注册、心跳、指数退避重连 |
| **移动眼** | MAVLink 飞控接入 | ✅ 完成 | PX4 / ArduPilot 自动识别、遥测聚合、指令与 ACK |
| **移动眼** | 遥测影子与事件流 | ✅ 完成 | 去重/乱序过滤、EventService 广播、指令超时回收 |
| **移动眼** | 前端地图随动 | ✅ 完成 | 2D/3D 视图与轨迹回放 |
| **移动眼** | 控制权仲裁与围栏 | ✅ 完成 | Redis Lua 租约跨实例互斥、指令状态机、围栏风控与告警去抖 |
| **传输** | TCP Server | ✅ 完成 | TCP传输模式 |
| **传输** | UDP Server | ✅ 完成 | UDP传输模式 |
| **传输** | TLS/SSL | 🔄 部分 | 客户端（ONVIF 设备侧、桌面端）支持 https/wss；服务端未内置 TLS 监听，需前置反代终止 |

## 项目结构

```
RUSTCAM/
├── crates/                     # Rust workspace
│   ├── rustcam-media/          # 媒体与信令库：HTTP API、协议、领域模型
│   │   └── src/
│   ├── rustcam-server/         # 服务端可执行程序
│   ├── rustcam-drone-service/  # 移动视点控制面（独立进程）
│   ├── rustcam-drone-adapter/  # 机载适配：飞控桥接与注册
│   ├── rustcam-drone-sim/      # MAVLink 机体模拟器（测试/演示）
│   ├── rustcam-drone-proto/    # 移动视点 gRPC 契约
│   └── rustcam-drone-core/     # 共享类型与常量
├── frontend/                   # Vue 前端
│   └── src/
│       └── views/drone/        # 无人机 / 实时飞行 / 航线 / 围栏 / 告警审计
├── testenv/                    # 移动眼测试环境（启动/停止脚本、截图）
│   ├── start.ps1
│   ├── stop.ps1
│   └── README.txt
├── clientapp/                  # Qt 桌面版
│   └── src/
├── public/                     # 对外文档
│   ├── api.md                  # API 参考
│   ├── architecture.mmd        # 架构图
│   └── architecture.png        # 架构图
├── sql/                        # 数据库脚本
└── CONTRIBUTING.md             # 贡献指南
```

## 部署

### 环境要求

| 组件 | 要求 |
|------|------|
| Rust | 1.95+（依赖树 MSRV：`parking_lot 0.13` / `lock_api 0.5` 为 1.95，`tonic` / `redis` / `config` 为 1.88） |
| PostgreSQL | 14+（**需 PostGIS 扩展**，通道坐标依赖 `geometry` 类型） |
| TimescaleDB | PostgreSQL 扩展（遥测时序归档必需，需在 `shared_preload_libraries` 中预加载） |
| Redis | 6+ |
| Node.js | 18+ (前端构建) |
| ZLMediaKit | 任意近期版本（推流 / 拉流 / 转推必需，配置模板见 `conf/config.ini`） |
| Qt + CMake | Qt 5.15.2（推荐）/ Qt 6.2+、CMake 3.16+、Visual Studio 2019（仅桌面客户端） |

### 配置文件

`config.toml` 从**工作目录**读取；文件不存在时后端会自动生成一份默认配置。下面是**可启动的最小完整配置**：

```toml
[server]
host = "0.0.0.0"
port = 8081
tls_enabled = false

[database]
url = "postgres://user:pass@localhost:5432/rustcam"
max_connections = 10

[redis]
url = "redis://localhost:6379"
pool_size = 5

[[media_servers.servers]]
name = "zlmediakit"
url = "http://127.0.0.1:80"
api_key = "your_api_key"               # 须与 ZLMediaKit config.ini 的 [api]secret 一致
server_type = "zlmediakit"             # zlmediakit / srs / xiu
weight = 100
enabled = true
server_tag = "default"

[cluster]
strategy = "round_robin"
health_check_interval_secs = 30
max_session_per_server = 1000

[session]
expiration_secs = 300
max_per_user = 10

[onvif]
discovery_enabled = false
scopes = ["onvif://www.onvif.org/Profile/Streaming", "onvif://www.onvif.org/Profile/PTZ"]

[rtsp_auth]
enabled = true
realm = "RustCam"

[signaling_server]
server_gb_id = "31011500001000000001"
server_gb_domain = "3101150000"
bind_ip = "0.0.0.0"                    # GB28181 联调填本机内网 IP，否则设备回推 RTP 失败
tcp_signaling_port = 5060
udp_signaling_port = 5061
rtp_port_start = 15000
rtp_port_end = 15100
rtp_bind_addr = "0.0.0.0"              # 同上，填本机内网 IP

[jwt]
secret = "change-me-in-production"     # 生产环境务必修改
access_token_expire_minutes = 60
refresh_token_expire_days = 30

# 移动眼控制面地址（缺省 http://127.0.0.1:50051）
[drone]
service_addr = "http://127.0.0.1:50051"
```

> `[cluster]` / `[session]` / `[onvif]` / `[rtsp_auth]` / `[signaling_server]` / `[jwt]` 六段为**必填**，
> 缺任一段后端启动即报 `Failed to deserialize config: missing configuration field "..."`。
>
> 可选段：`[log]`（级别与目录，默认 `./logs`）、`[rtsp_server]`（TCP RTSP 信令，默认 `0.0.0.0:554`）、
> `[recovery]`（流恢复退避策略）、`[media_servers.servers.protocol_ports]`（各协议端口，须与 ZLMediaKit
> `config.ini` 保持一致）。

### 数据库初始化

```bash
# 创建数据库（通道坐标依赖 PostGIS：schema.sql 内含 CREATE EXTENSION postgis，
# 需要超级用户权限，或由 DBA 预先装好扩展）
psql -U postgres -c "CREATE DATABASE rustcam;"
psql -U postgres -d rustcam -f sql/schema.sql

# 角色继承（角色层级授权）：role_inheritance 表不在 schema.sql 中，需单独执行
psql -U postgres -d rustcam -f sql/migrate_role_inheritance.sql
```

> - `sql/schema.sql` 是唯一的建库脚本；后续结构变更以 `sql/migrate_*.py` 追加，在 `sql/` 目录按需执行。
> - 移动眼相关表（`drone_device`、`drone_geofence`、`drone_control_audit` 等）由 `rustcam-drone-service`
>   启动时自动执行 `crates/rustcam-drone-service/migrations.sql` 创建，无需手工建表。
> - 遥测时序数据写入 TimescaleDB 超表 `drone_telemetry`（`migrations.sql` 自动执行 `CREATE EXTENSION timescaledb`）。

### 构建与运行

```bash
# 构建后端（仓库根目录是虚拟 workspace，需用 -p 指定包）
cargo build --release -p rustcam-server
./target/release/rustcam-media

# 或开发模式
cargo run -p rustcam-server
```

> 在仓库根目录直接执行 `cargo run` 会失败：`could not determine which binary to run`，
> 因为 workspace 内有多个二进制目标，须用 `-p` / `--bin` 指定。

移动眼（可选，独立进程）：

```bash
# 控制面：读取工作目录的 drone.toml，gRPC :50051 / metrics :50052
cargo run -p rustcam-drone-service

# 机载适配：读取工作目录的 adapter.toml（示例见 adapter.toml.example）
# 默认连接 udpin:0.0.0.0:14540（PX4 SITL）
cargo run -p rustcam-drone-adapter
```

> 主服务需通过 `config.toml` 的 `[drone].service_addr` 指向控制面；控制面未启动时主服务仍可正常启动，
> 仅 `/api/v1/drone/*` 返回 503（`drone-service 未配置`）。

### TLS/HTTPS 部署

平台后端**不内置 TLS 监听**：`config.toml` 里的 `tls_enabled` / `tls_cert` / `tls_key` 目前只是预留字段，
HTTP 服务为明文（`crates/rustcam-server/src/main.rs` 直接 `axum::serve`）。HTTPS / WSS 请由前置反向代理终止。

1. 生成自签名证书（测试用）:
```bash
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost"
```

2. 或使用 Let's Encrypt 证书（生产用）:
```bash
certbot certonly --nginx -d yourdomain.com
```

3. 由反向代理加载证书，并反代到后端 `8081`。前端控制台需先构建（`cd frontend && npm install && npm run build`，
产物在 `frontend/dist`），后端不提供静态文件服务：

```nginx
server {
    listen 443 ssl;
    server_name yourdomain.com;

    ssl_certificate     /etc/letsencrypt/live/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/yourdomain.com/privkey.pem;

    root /opt/rustcam/frontend/dist;      # 前端静态产物
    index index.html;

    location /api/ {
        proxy_pass http://127.0.0.1:8081;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location /ws {
        proxy_pass http://127.0.0.1:8081;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
    }
}
```

> 页面以 HTTPS 提供时，浏览器会自动使用 `wss://` 连接 `/ws`（见 `frontend/src/composables/useWsConnection.js`），
> 因此代理必须支持 WebSocket 升级；桌面客户端在登录框勾选「Use HTTPS」即可走同一入口。

### Docker 部署（可选）

仓库当前**不提供 Dockerfile 与镜像**，平台本体按「二进制 + 配置文件」方式部署；依赖服务可用容器运行。
注意官方 `postgres` 镜像不含 PostGIS，需改用 `postgis/postgis` 镜像（tag 形如 `<PG大版本>-<PostGIS版本>`）：

```bash
# 自定义网络，便于容器间按主机名互访
docker network create rustcam-net

# PostgreSQL + PostGIS
docker run -d --name postgres --network rustcam-net \
  -e POSTGRES_DB=rustcam \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=your_password \
  -v postgres_data:/var/lib/postgresql/data \
  postgis/postgis:16-3.4

# Redis
docker run -d --name redis --network rustcam-net \
  -v redis_data:/data \
  redis:7-alpine
```

此时 `config.toml` 中数据库地址写 `postgres://postgres:your_password@postgres:5432/rustcam`，
Redis 写 `redis://redis:6379`。

## 项目进度

- [x] GB28181 设备接入 (UDP/TCP)
- [x] ONVIF 设备发现与控制
- [x] RTSP 流接收
- [x] 云台 PTZ 控制 (GB28181 + ONVIF)
- [x] 前端 PTZ UI 重构 (控制面板 + 预置位分离)
- [x] 统一 API 响应格式 (中文错误消息)
- [x] 电子地图 (MapLibre GL JS + GeoServer)
- [ ] SRS 流媒体支持
- [ ] XIU 流媒体支持
- [ ] HTTPS/WSS 加密传输（TLS 需由前置反代终止，服务端未内置 TLS 监听）
- [x] 移动眼三进程骨架 (drone-service / drone-adapter / 网关)
- [x] 角色继承与数据库驱动的权限策略
- [x] 真实 MAVLink 飞控接入（PX4 / ArduPilot，遥测 + 指令 + ACK）
- [x] 移动眼前端视图 (2D 地图 + 3D 姿态)
- [x] 控制权仲裁、指令状态机、围栏风控

## 贡献指南

欢迎参与 RustCam-Media。开发环境、本地启动、代码规范、提交规范与 PR 流程详见 **[`CONTRIBUTING.md`](./CONTRIBUTING.md)**。

快速开始：

```bash
# 后端
cargo build && cargo run -p rustcam-server

# 前端
cd frontend && npm install && npm run dev
```

提交代码前请至少执行：

```bash
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

提交信息采用 Conventional Commits 规范：

```
feat(frontend): 通道编辑弹窗接入地图选点
fix(device): 修复设备按 tag 更新时坐标未迁移的问题
```

> 新增或修改接口后，请同步更新 [`public/api.md`](./public/api.md)；架构调整时修改 `public/architecture.mmd` 并执行 `node public/render-architecture.mjs` 重新生成架构图。

## License

MIT License - 详见 [LICENSE](./LICENSE)
