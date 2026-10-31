# RUSTCAM

现代化视频监控网关与信令控制平台，基于 Rust 后端 + Vue 前端构建，支持多协议设备接入、实时视频播放、录像回放、云台控制等功能。

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
| 用户权限 | JWT 认证、RBAC 权限控制 |

## 技术栈

### 后端 (Rust)

| 层级 | 技术 |
|------|------|
| 协议 | RTSP、GB28181、ONVIF、WebSocket |
| 认证 | JWT + Casbin |
| 存储 | Redis (缓存) + PostgreSQL (持久化) |

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
| **传输** | TCP Server | ✅ 完成 | TCP传输模式 |
| **传输** | UDP Server | ✅ 完成 | UDP传输模式 |
| **传输** | TLS/SSL | ✅ 完成 | HTTPS/WSS 支持，rustls 实现 |

## 项目结构

```
RUSTCAM/
├── src/                    # Rust 后端
│   ├── api/               # HTTP API
│   ├── application/       # 应用服务
│   ├── auth/              # 认证授权
│   ├── domain/            # 数据模型
│   ├── infrastructure/    # 基础设施
│   ├── protocol/          # 协议实现
│   └── transport/         # 网络传输
├── frontend/              # Vue 前端
│   └── src/
├── clientapp/             # Qt 桌面版
│   └── src/
├── public/                # 对外文档
│   ├── api.md             # API 参考
│   ├── architecture.mmd   # 架构图
│   └── architecture.png   # 架构图
├── sql/                   # 数据库脚本
├── tests/                 # 测试用例
└── CONTRIBUTING.md        # 贡献指南
```

## 部署

### 环境要求

| 组件 | 要求 |
|------|------|
| Rust | 1.70+ |
| PostgreSQL | 14+ |
| Redis | 6+ |
| Node.js | 18+ (前端构建) |

### 配置文件

编辑 `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 8081
tls_enabled = false                    # 启用 TLS
tls_cert = "/path/to/cert.pem"        # TLS 证书
tls_key = "/path/to/key.pem"          # TLS 私钥

[database]
url = "postgres://user:pass@localhost:5432/rustcam"
max_connections = 10

[redis]
url = "redis://localhost:6379"
pool_size = 5

[[media_servers.servers]]
name = "zlmediakit"
url = "http://127.0.0.1:80"
api_key = "your_api_key"
server_type = "zlmediakit"            # zlmediakit / srs / xiu
weight = 100
enabled = true
server_tag = "default"

[jwt]
secret = "change-me-in-production"     # 生产环境务必修改
```

### 数据库初始化

```bash
# 创建数据库
psql -U postgres -c "CREATE DATABASE rustcam;"
psql -U postgres -d rustcam -f sql/schema.sql
```

### 构建与运行

```bash
# 构建后端
cargo build --release
./target/release/rustcam-media

# 或开发模式
cargo run
```

### TLS/HTTPS 部署

1. 生成自签名证书（测试用）:
```bash
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost"
```

2. 或使用 Let's Encrypt 证书（生产用）:
```bash
certbot certonly --nginx -d yourdomain.com
```

3. 配置 `config.toml`:
```toml
[server]
tls_enabled = true
tls_cert = "/etc/letsencrypt/live/yourdomain.com/fullchain.pem"
tls_key = "/etc/letsencrypt/live/yourdomain.com/privkey.pem"
```

### Docker 部署（可选）

```bash
# PostgreSQL
docker run -d --name postgres \
  -e POSTGRES_DB=rustcam \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=your_password \
  -v postgres_data:/var/lib/postgresql/data \
  postgres:14

# Redis
docker run -d --name redis \
  -v redis_data:/data \
  redis:7-alpine

# RustCam
docker build -t rustcam .
docker run -d --name rustcam \
  --link postgres \
  --link redis \
  -p 8081:8081 \
  -v $(pwd)/config.toml:/app/config.toml \
  rustcam
```

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
- [x] HTTPS/WSS 加密传输

## 贡献指南

欢迎参与 RustCam-Media。开发环境、本地启动、代码规范、提交规范与 PR 流程详见 **[`CONTRIBUTING.md`](./CONTRIBUTING.md)**。

快速开始：

```bash
# 后端
cargo build && cargo run

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
