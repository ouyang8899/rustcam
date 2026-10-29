# RustCam-Media API 参考

后端 HTTP / WebSocket 接口说明，覆盖 `src/api/http/routes.rs` 中注册的全部 **102 条受保护路由** 与 **14 条公开路由**。

- 服务框架：Axum 0.8 + Tokio
- 默认监听：`0.0.0.0:8081`（见 `config.toml` 的 `[server].port`）
- 业务接口统一前缀：`/api/v1`
- 请求与响应均为 `application/json`（`GET /api/v1/metrics` 除外，返回 Prometheus 文本）
- 当前仓库 **未提供 OpenAPI / Swagger 描述文件**，本文件即接口契约的唯一来源

---

## 目录

- [认证](#认证)
- [统一响应格式](#统一响应格式)
- [分页与筛选约定](#分页与筛选约定)
- [接口清单](#接口清单)
  - [系统与监控](#系统与监控)
  - [认证与用户权限](#认证与用户权限)
  - [设备管理](#设备管理)
  - [通道管理](#通道管理)
  - [云台控制（PTZ）](#云台控制ptz)
  - [流与播放](#流与播放)
  - [会话](#会话)
  - [录像管理](#录像管理)
  - [录像策略](#录像策略)
  - [录像文件与回放](#录像文件与回放)
  - [报警](#报警)
  - [媒体服务器](#媒体服务器)
  - [监控墙布局](#监控墙布局)
  - [设备分组](#设备分组)
  - [行政区划](#行政区划)
  - [GB28181 参考数据](#gb28181-参考数据)
  - [地图设置](#地图设置)
  - [ONVIF](#onvif)
  - [公开直播](#公开直播)
  - [WebSocket](#websocket)
  - [媒体服务器回调（Hook）](#媒体服务器回调hook)
- [调用示例](#调用示例)
- [已知问题与注意事项](#已知问题与注意事项)

---

## 认证

除「公开路由」一节列出的端点外，所有 `/api/v1/*` 业务接口都需要携带 **JWT**：

```http
Authorization: Bearer <access_token>
```

- 仅支持 `Authorization` 头，**不支持** Cookie、URL 查询参数
- `Bearer` 前缀大小写敏感，后跟一个空格
- 令牌算法：HS256；载荷含 `sub`(用户 UUID)、`username`、`roles`、`exp`、`iat`、`token_type`
- `token_type` 必须是 `access`（`refresh` 令牌只能用于刷新接口）

### 登录

```http
POST /api/v1/auth/login
Content-Type: application/json

{ "username": "admin", "password": "******" }
```

响应 `data`：

```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_in": 3600,
  "token_type": "Bearer",
  "user": { "id": "6f1c...", "username": "admin", "roles": ["admin"] }
}
```

口令使用 **Argon2** 校验（`argon2::Argon2::default()`），口令明文不落库。

### 刷新令牌

```http
POST /api/v1/auth/refresh

{ "refresh_token": "eyJhbGciOiJIUzI1NiIs..." }
```

返回结构与登录一致（同时下发新的 access 与 refresh 令牌）。刷新令牌对应用户必须仍然存在。

### 当前用户

```http
GET /api/v1/auth/me
```

### 令牌有效期与密钥

| 配置项 | 来源 | 默认值 |
|--------|------|--------|
| 签名密钥 | `config.toml` 的 `[jwt].secret`；若该段缺省则读环境变量 `JWT_SECRET` | `dev-secret-change-in-production`（**生产环境必须覆盖**） |
| access 有效期 | `[jwt].access_token_expire_minutes` | 60 分钟 |
| refresh 有效期 | `[jwt].refresh_token_expire_days` | 30 天 |

### 关于 RBAC

项目使用 **Casbin** 做权限建模（模型见 `casbin_model.conf`，策略见 `casbin_policy.csv`），用户 / 角色 / 权限的数据模型与增删改查接口均已落库到 PostgreSQL。

> ⚠️ **当前版本尚未在 HTTP 层强制执行权限校验**：`CasbinManager::enforce()` 尚未被任何中间件或处理器调用，策略文件目前也只有通配占位。因此只要持有合法 access token，即可访问全部受保护接口（包括用户与权限管理接口）。详见[已知问题](#已知问题与注意事项)。

---

## 统一响应格式

除少数例外，所有接口都返回统一封装：

```json
{ "code": 200, "message": "success", "data": { } }
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `code` | number | 业务码，成功固定为 `200` |
| `message` | string | 成功为 `"success"`，失败为中文错误描述 |
| `data` | any | 业务数据，失败时为 `null` |

### 错误码

| HTTP | `code` | 典型场景 |
|------|--------|----------|
| 400 | 400 | 参数非法（缺少必填字段、枚举值非法、分页越界） |
| 401 | 401 | 缺少 / 无效 / 过期的 Authorization 头，口令错误 |
| 403 | 403 | 权限不足（模型已预留，当前未启用） |
| 404 | 404 | 设备、通道、流、录像、角色等资源不存在 |
| 500 | 500 | 内部错误、数据库错误 |
| 502 | 502 | 媒体服务器（ZLMediaKit / SRS / XIU）调用失败 |
| 503 | 503 | 设备离线（例如 GB28181 云台控制） |

### 不遵循统一封装的端点

| 端点 | 实际返回 |
|------|----------|
| `GET /` | 纯文本 `RustCam-Media API v2.0` |
| `GET /api/v1/metrics` | `text/plain` Prometheus 指标 |
| `GET /api/v1/stats` | 裸 JSON `{ "devices": {...}, "streams": {...} }`（无 `code`/`message` 包装） |
| `GET /api/v1/proxy/wfs` | 透传的原始字节（GeoServer WFS 响应） |
| `/hook/*`、`POST /alarm/{stream_key}` | `{"code": 0, "msg": "ok"}` —— **注意 `code` 为 `0`，字段名是 `msg` 而非 `message`** |
| `POST /api/v1/admin/reload-cache` | 失败时 HTTP 仍为 200，错误体现在响应体 `code: 500` |

---

## 分页与筛选约定

列表接口目前存在 **三套并存的**分页风格，调用时需按接口区分：

### 风格 A — `limit` / `offset`（最常见）

默认值 `limit=50`，上限 `500`（超出后静默截断，不报错）。

| 接口 | 筛选参数 |
|------|----------|
| `GET /api/v1/devices` | `search`、`region_code`、`protocol`、`status`、`device_type`、`group_id`、`limit`、`offset` |
| `GET /api/v1/devices/online` | `limit`、`offset` |
| `GET /api/v1/channels` | `device_tag`、`search`、`status`、`limit`、`offset` |
| `GET /api/v1/streams` | `search`、`state`、`limit`、`offset` |
| `GET /api/v1/sessions` | `search`、`state`、`limit`、`offset` |
| `GET /api/v1/recordings` | `device_tag`、`channel_tag`、`state`、`search`、`limit`、`offset` |
| `GET /api/v1/recording-policies` | `device_tag`、`policy_type`、`limit`、`offset` |

响应：

```json
{ "items": [], "total": 123, "limit": 50, "offset": 0 }
```

### 风格 B — `page` / `page_size`（仅报警）

`page` 默认 `1`，`page_size` 默认 `20`、上限 `100`。

```http
GET /api/v1/alarms?device_tag=xxx&page=1&page_size=20
```

```json
{ "items": [], "total": 62, "page": 1, "page_size": 20 }
```

### 风格 C — `page` / `limit`（仅录像文件）

`page` 必须 ≥ 1，`limit` 取值 1–500，**越界返回 HTTP 400**。

```json
{ "files": [], "total": 0, "page": 1, "limit": 50, "has_more": false }
```

### 通用约定

- 文本搜索参数统一叫 **`search`**（不是 `keyword`）
- 设备列表用 `status`，流 / 会话 / 录像列表用 `state`
- 时间参数为 RFC3339，例如 `2026-10-29T12:00:00Z`
- 空字符串筛选值视为未传
- 设备协议枚举：`Gb28181` / `Onvif` / `Rtsp` / `Rtmp` / `Hls` / `WebRTC` / `Custom`
- 播放协议枚举（小写）：`rtsp` / `rtmp` / `hls` / `http` / `webrtc` / `flv` / `ws_flv`

---

## 接口清单

下表中 **鉴权** 列：🔒 = 需要 JWT，🔓 = 公开。

### 系统与监控

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/` | 服务标识，返回纯文本 `RustCam-Media API v2.0` | 🔓 |
| GET | `/api/v1/health` | 健康检查，`data` 为 `{"status":"ok"}` | 🔓 |
| GET | `/api/v1/metrics` | Prometheus 指标（text/plain） | 🔓 |
| GET | `/api/v1/stats` | 设备 / 流统计（裸 JSON） | 🔓 |
| GET | `/api/v1/dashboard` | 仪表盘汇总数据 | 🔒 |
| POST | `/api/v1/admin/reload-cache` | 重载设备 / 通道缓存 | 🔒 |

### 认证与用户权限

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| POST | `/api/v1/auth/login` | 登录，下发 access / refresh 令牌 | 🔓 |
| POST | `/api/v1/auth/refresh` | 刷新令牌 | 🔓 |
| GET | `/api/v1/auth/me` | 当前登录用户信息 | 🔒 |
| GET | `/api/v1/users` | 用户列表（`password_hash` 已被掩码为 `***`） | 🔒 |
| POST | `/api/v1/users` | 创建用户，请求体 `{username, password, role_ids?}` | 🔒 |
| GET | `/api/v1/users/{id}` | 用户详情（`id` 为 UUID），返回 `UserWithRoles` | 🔒 |
| PUT | `/api/v1/users/{id}` | 更新用户，请求体 `{username?, password?}` | 🔒 |
| DELETE | `/api/v1/users/{id}` | 删除用户 | 🔒 |
| PUT | `/api/v1/users/{id}/roles` | 分配角色，请求体 `{role_ids: [uuid]}` | 🔒 |
| GET | `/api/v1/roles` | 角色列表 | 🔒 |
| POST | `/api/v1/roles` | 创建角色，请求体 `{name, description?, permission_ids?}` | 🔒 |
| PUT | `/api/v1/roles/{id}/permissions` | 设置角色权限，请求体 `{permission_ids: [uuid]}` | 🔒 |
| GET | `/api/v1/permissions` | 权限列表 | 🔒 |
| POST | `/api/v1/permissions` | 创建权限，请求体 `{name, description?}` | 🔒 |

> 当前未提供角色的修改 / 删除接口，也没有权限的修改 / 删除接口。

### 设备管理

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/devices` | 设备列表（分页） | 🔒 |
| GET | `/api/v1/devices/online` | 在线设备列表（分页） | 🔒 |
| POST | `/api/v1/devices` | 创建设备 | 🔒 |
| GET | `/api/v1/devices/{id}` | 设备详情。**`{id}` 为 device_tag 字符串** | 🔒 |
| PUT | `/api/v1/devices/{id}` | 更新设备。**`{id}` 为数字数据库 ID** | 🔒 |
| PUT | `/api/v1/devices/by-tag/{device_tag}` | 按 tag 更新设备（tag 版别名） | 🔒 |
| DELETE | `/api/v1/devices/{id}` | 删除设备（`{id}` 为 device_tag），同时停止流并清理录像 | 🔒 |
| POST | `/api/v1/devices/{id}/play` | 播放设备，查询参数 `?protocol=`，返回 `StreamInfo` | 🔒 |
| POST | `/api/v1/devices/{id}/playback` | 设备回放，请求体 `{start_time, end_time}`（RFC3339），仅 GB28181 | 🔒 |
| GET | `/api/v1/devices/{id}/config` | 获取设备配置，查询参数 `?type=`（默认 `BasicParam`），返回原始 XML | 🔒 |
| POST | `/api/v1/devices/{id}/stop` | 停止该设备上的活动会话 | 🔒 |
| POST | `/api/v1/devices/{id}/start` | 启动设备，查询参数 `?channel_tag=` | 🔒 |
| GET | `/api/v1/devices/{id}/channels` | 设备下的通道列表（`?status=` 暂未生效） | 🔒 |

### 通道管理

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/channels` | 通道列表（分页） | 🔒 |
| POST | `/api/v1/channels` | 创建通道 | 🔒 |
| GET | `/api/v1/channels/{device_tag}/{channel_tag}` | 通道详情 | 🔒 |
| PUT | `/api/v1/channels/{device_tag}/{channel_tag}` | 更新通道（经纬度等字段在通道上） | 🔒 |
| GET | `/api/v1/channels/{device_tag}/{channel_tag}/play-links` | 播放地址集合，查询参数 `?stream_key=` | 🔒 |
| GET | `/api/v1/channels/{device_tag}/{channel_tag}/status` | 通道状态 `{channel_tag, device_tag, name, status, has_stream, stream_count}` | 🔒 |
| POST | `/api/v1/channels/{device_tag}/{channel_tag}/start` | 启动通道拉流，仅 GB28181 | 🔒 |

### 云台控制（PTZ）

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| POST | `/api/v1/channels/{device_tag}/{channel_tag}/ptz` | 云台控制，返回 `PtzControlResult` | 🔒 |
| GET | `/api/v1/channels/{device_tag}/{channel_tag}/ptz/presets` | 预置位列表 | 🔒 |
| POST | `/api/v1/channels/{device_tag}/{channel_tag}/ptz/presets` | 新建预置位，请求体 `{name}` | 🔒 |
| PUT | `/api/v1/channels/{device_tag}/{channel_tag}/ptz/presets/{token}` | 重命名预置位，请求体 `{name}` | 🔒 |
| DELETE | `/api/v1/channels/{device_tag}/{channel_tag}/ptz/presets/{token}` | 删除预置位 | 🔒 |
| GET | `/api/v1/channels/{device_tag}/{channel_tag}/ptz/status` | 云台状态 `{position_pan, position_tilt, position_zoom, moving}` | 🔒 |

PTZ 请求体：

```json
{
  "move_type": "continuous",
  "command": "up",
  "speed": 50,
  "pan": 0.0, "tilt": 0.0, "zoom": 0.0,
  "preset_token": null, "preset_name": null, "channel": 8
}
```

- `move_type`：`continuous` / `absolute` / `relative` / `goto_preset` / `set_preset` / `remove_preset` / `stop`
- `command`：`up` / `down` / `left` / `right` / `zoom_in` / `zoom_out` / `focus_in` / `focus_out` / `iris_in` / `iris_out` / `stop`
- `move_type` 与 `command` 至少提供一个；`goto_preset` / `remove_preset` 必须带 `preset_token`
- `speed` 取值 0–100，默认 50
- 设备离线时返回 **503**

> 仅提供**通道级** PTZ 接口；设备级（`/api/v1/devices/{id}/ptz`）的处理器已实现但未注册路由。

### 流与播放

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/streams` | 流列表（分页） | 🔒 |
| POST | `/api/v1/streams` | 启动拉流，请求体 `{device_tag, channel_tag?, rtsp_url?}` | 🔒 |
| GET | `/api/v1/streams/{id}` | 流详情（`{id}` 为数字 ID） | 🔒 |
| DELETE | `/api/v1/streams/{id}` | 停止流 | 🔒 |
| GET | `/api/v1/streams/{id}/play` | 播放地址，`?protocol=` **必填**，返回 URL 字符串 | 🔒 |
| GET | `/api/v1/streams/{id}/play-links` | 完整播放地址集合 `PlayLinks` | 🔒 |
| GET | `/api/v1/streams/{id}/online` | 流是否在线，返回布尔值 | 🔒 |
| POST | `/api/v1/streams/{id}/restart` | 重启流 | 🔒 |
| GET | `/api/v1/streams/by-device/{device_id}` | 按设备查询流。**路径参数名为 `device_id`，实际是 device_tag 字符串** | 🔒 |

`StreamInfo` 字段：`stream_key`、`play_url`、`rtsp_url`、`rtmp_url`、`hls_url`、`webrtc_url`、`flv_url?`、`web_flv_url?`、`media_server_id`、`media_server_name`。

### 会话

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/sessions` | 会话列表（分页） | 🔒 |

> 会话由 `POST /api/v1/devices/{id}/play` 等操作隐式创建，没有独立的创建 / 关闭 / 刷新接口。

### 录像管理

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/recordings` | 录像任务列表（分页） | 🔒 |
| POST | `/api/v1/recordings` | 创建录像任务，请求体 `{device_tag, channel_tag?, format?, duration_secs?, max_file_size_mb?, output_path?, labels?, pre_record_secs?}` | 🔒 |
| GET | `/api/v1/recordings/stats` | 录像统计 | 🔒 |
| GET | `/api/v1/recordings/{id}` | 录像任务详情 | 🔒 |
| POST | `/api/v1/recordings/{id}/start` | 开始录像 | 🔒 |
| POST | `/api/v1/recordings/{id}/stop` | 停止录像 | 🔒 |
| POST | `/api/v1/recordings/{id}/pause` | 暂停录像 | 🔒 |
| POST | `/api/v1/recordings/{id}/resume` | 恢复录像 | 🔒 |
| DELETE | `/api/v1/recordings/{id}` | 删除录像任务 | 🔒 |
| GET | `/api/v1/recordings/{id}/files` | 该任务下的录像文件 | 🔒 |

枚举值：`RecordingFormat` = `Hls` / `Mp4` / `Flv` / `Ts`；`RecordingState` = `Pending` / `Starting` / `Recording` / `Paused` / `Stopping` / `Completed` / `Error`；`RecordingType` = `timed` / `alarm` / `manual`。

### 录像策略

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/recording-policies` | 策略列表（分页，`?device_tag`、`?policy_type`） | 🔒 |
| POST | `/api/v1/recording-policies` | 创建策略 | 🔒 |
| GET | `/api/v1/recording-policies/{id}` | 策略详情 | 🔒 |
| PUT | `/api/v1/recording-policies/{id}` | 更新策略 | 🔒 |
| DELETE | `/api/v1/recording-policies/{id}` | 删除策略 | 🔒 |

`PolicyType` = `Timed` / `AlarmTriggered`；`ScheduleType` = `Cron` / `TimeWindows`。策略字段包含 `cron_expr`、`alarm_type`、`pre_record_secs`、`post_record_secs`、`format`、`max_duration_secs`、`max_file_size_mb`、`output_path`、`time_windows[]`。

### 录像文件与回放

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/recordings/files` | 录像文件检索，参数 `device_tag?`、`channel_tag?`、`start_time?`、`end_time?`、`segment_type?`、`limit?`、`page?` | 🔒 |
| GET | `/api/v1/recordings/files/{id}/play-url` | 录像文件播放地址，`{id}` 为字符串文件 ID | 🔒 |

`SegmentType` = `normal` / `alarm` / `prebuffer`（也接受 `pre-buffer`）。

### 报警

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/alarms` | 报警记录，参数 `device_id?`、`device_tag?`、`channel_tag?`、`alarm_type?`、`start_time?`、`end_time?`、`page?`、`page_size?` | 🔒 |
| PUT | `/api/v1/alarms/{id}/processed` | 标记处理状态，请求体 `{processed: bool}` | 🔒 |

### 媒体服务器

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/servers` | 媒体服务器列表（含实时状态） | 🔒 |
| POST | `/api/v1/servers` | 添加服务器，请求体 `{name, url, api_key, server_type, weight?, server_tag, protocol_ports?}` | 🔒 |
| GET | `/api/v1/servers/{tag}` | 服务器详情，`{tag}` 为 server_tag | 🔒 |
| PUT | `/api/v1/servers/{tag}` | 更新服务器 | 🔒 |
| DELETE | `/api/v1/servers/{tag}` | 删除服务器 | 🔒 |
| GET | `/api/v1/servers/{tag}/status` | 运行状态 `{online, session_count, cpu_usage, memory_usage}` | 🔒 |
| POST | `/api/v1/servers/{tag}/refresh` | 刷新状态，返回 `{online, session_count}` | 🔒 |
| POST | `/api/v1/servers/{tag}/enable` | 启用服务器 | 🔒 |
| POST | `/api/v1/servers/{tag}/disable` | 禁用服务器 | 🔒 |
| GET | `/api/v1/servers/{tag}/sessions` | 服务器上的会话列表 | 🔒 |

`server_type`：`zlmediakit` / `srs` / `xiu`。`protocol_ports` 含 `rtsp`、`rtmp`、`hls`、`http`、`https`、`webrtc`、`rtp_tcp`、`http_flv`、`ws_flv`。

### 监控墙布局

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/layouts` | 布局列表 | 🔒 |
| POST | `/api/v1/layouts` | 创建布局，请求体 `{name, rows, cols, layout_json[], is_default}` | 🔒 |
| GET | `/api/v1/layouts/{id}` | 布局详情（`{id}` 为 i32） | 🔒 |
| PUT | `/api/v1/layouts/{id}` | 更新布局 | 🔒 |
| DELETE | `/api/v1/layouts/{id}` | 删除布局 | 🔒 |
| PUT | `/api/v1/layouts/{id}/default` | 设为默认布局 | 🔒 |

`LayoutItem` = `{id, row, col, row_span, col_span, label?}`。

### 设备分组

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/groups/tree` | 分组树 | 🔒 |
| GET | `/api/v1/groups` | 分组列表 | 🔒 |
| POST | `/api/v1/groups` | 创建分组，请求体 `{name, parent_id?, sort_order?}` | 🔒 |
| PUT | `/api/v1/groups/{id}` | 更新分组 | 🔒 |
| DELETE | `/api/v1/groups/{id}` | 删除分组 | 🔒 |
| PUT | `/api/v1/devices/{id}/group` | 分配设备分组，请求体 `{group_id}`。**`{id}` 为数字设备 ID** | 🔒 |

### 行政区划

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/regions` | 行政区划列表，`?parent=` 指定父级编码（缺省返回根节点） | 🔒 |
| GET | `/api/v1/regions/tree` | 行政区划树 | 🔒 |

`Region` = `{code, name, level, parent_code?, province_name?, city_name?, district_name?, device_count}`。

### GB28181 参考数据

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/gb28181/ref-data` | 返回 `{device_types[], industry_codes[], network_codes[]}` | 🔒 |

### 地图设置

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/settings/map` | 读取地图配置 | 🔒 |
| PUT | `/api/v1/settings/map` | 更新地图配置（字段均可选） | 🔒 |

`MapSettings` = `{browser_tms_url, client_tms_url, wfs_url, browser_min_zoom, browser_max_zoom, client_min_zoom, client_max_zoom, updated_at}`。

### ONVIF

| 方法 | 路径 | 请求体 | 说明 | 鉴权 |
|------|------|--------|------|:----:|
| POST | `/api/v1/onvif/discover` | — | WS-Discovery 组播探测 | 🔒 |
| POST | `/api/v1/onvif/probe` | `{host, port}` | 探测设备 | 🔒 |
| POST | `/api/v1/onvif/capabilities` | `{x_addr, username, password}` | 查询设备能力 | 🔒 |
| POST | `/api/v1/onvif/stream-uris` | `{x_addr, media_x_addr?, username, password, profiles[]}` | 获取取流地址 | 🔒 |
| POST | `/api/v1/onvif/devices` | `{urn?, x_addr, name?, manufacturer?, model?, username, password, capabilities{...}, types[], scopes[], streams[]}` | 创建设备 | 🔒 |
| POST | `/api/v1/onvif/check-online` | `{x_addr, username, password}` | 在线检测，返回 `{online: bool}` | 🔒 |

### 公开直播

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET | `/api/v1/public/streams` | 公开设备（`is_public = true`）的流列表 | 🔓 |

### WebSocket

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET (WS) | `/ws` | 服务端事件广播（报警、流状态变化等） | 🔓 |
| GET (WS) | `/ws/audio-talk/{device_id}/{channel_id}` | 语音对讲通道，路径参数为 device_tag + channel_id | 🔓 |

**事件推送 `/ws`**：服务端主动推送 JSON 文本帧。

**语音对讲 `/ws/audio-talk/...`**：

- 客户端发送 JSON 文本帧 `{"action":"start_talk"}` / `{"action":"stop_talk"}`
- 服务端回复 `{"status":"ready"|"error","message":"..."}`
- 随后二进制帧承载线性 PCM（16 kHz），服务端转 G711 PCMA 经 GB28181 / ONVIF 反向通道下发
- 仅支持 **GB28181** 与 **ONVIF** 两种协议，其他协议会返回错误 JSON 后关闭连接

### 媒体服务器回调（Hook）

媒体服务器（ZLMediaKit 等）向本服务推送事件的入口。

| 方法 | 路径 | 说明 | 鉴权 |
|------|------|------|:----:|
| GET, POST | `/hook/{server_type}/{event_name}` | 标准形式，`server_type` ∈ `zlmediakit` / `srs` / `xiu` | 🔓 |
| GET, POST | `/hook/{event_name}` | 旧版单段形式，按 ZLMediaKit 事件解析 | 🔓 |
| GET, POST | `/hook` | 不带事件名 | 🔓 |
| POST | `/alarm/{stream_key}` | 外部告警上报，`stream_key` 形如 `deviceTag_channelTag` | 🔓 |
| GET | `/api/v1/proxy/wfs` | GeoServer WFS 透传代理，`?url=` **必填**，10 秒超时 | 🔓 |

所有 Hook 端点固定返回 `{"code": 0, "msg": "ok"}`。

ZLMediaKit 的 Hook 地址需要在 `conf/config.ini` 中配置，例如：

```ini
[hook]
enable = yes
on_stream_publish = http://127.0.0.1:8081/hook/zlmediakit/on_stream_publish
on_play = http://127.0.0.1:8081/hook/zlmediakit/on_play
on_stream_none_reader = http://127.0.0.1:8081/hook/zlmediakit/on_stream_none_reader
```

---

## 调用示例

### 1. 登录并保存令牌

```bash
BASE=http://127.0.0.1:8081

TOKEN=$(curl -s -X POST "$BASE/api/v1/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' \
  | sed -n 's/.*"access_token":"\([^"]*\)".*/\1/p')
```

### 2. 拉取设备列表

```bash
curl -s "$BASE/api/v1/devices?limit=20&offset=0&status=online" \
  -H "Authorization: Bearer $TOKEN"
```

```json
{
  "code": 200,
  "message": "success",
  "data": {
    "items": [
      {
        "id": 1,
        "device_id": "34020000001320000001",
        "device_tag": "onvif-test",
        "name": "onvif-test",
        "protocol": "Onvif",
        "status": "online"
      }
    ],
    "total": 1,
    "limit": 20,
    "offset": 0
  }
}
```

### 3. 启动拉流并取播放地址

```bash
STREAM=$(curl -s -X POST "$BASE/api/v1/streams" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"device_tag":"onvif-test","channel_tag":"ch1"}')

curl -s "$BASE/api/v1/streams/1/play?protocol=webrtc" \
  -H "Authorization: Bearer $TOKEN"
```

### 4. 云台控制

```bash
curl -s -X POST "$BASE/api/v1/channels/onvif-test/ch1/ptz" \
  -H "Authorization: Bearer $TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"command":"up","speed":50}'
```

### 5. 订阅事件（WebSocket）

```javascript
const ws = new WebSocket('ws://127.0.0.1:8081/ws')
ws.onmessage = (e) => console.log(JSON.parse(e.data))
```

---

## 已知问题与注意事项

1. **RBAC 未在 HTTP 层生效**：`CasbinManager::enforce()` 未被任何中间件或处理器调用，`AuthError::PermissionDenied`（403）也从未被构造。当前任何合法 access token 都能访问全部受保护接口，包括 `/api/v1/users`、`/api/v1/roles`、`/api/v1/permissions`。策略文件 `casbin_policy.csv` 目前也只有通配占位。

2. **`/api/v1/devices/{id}` 的路径参数语义不一致**：
   - `GET` / `DELETE` / `play` / `playback` / `config` / `stop` / `start` / `channels` 使用 **device_tag 字符串**
   - `PUT` 与 `PUT /api/v1/devices/{id}/group` 使用 **数字数据库 ID**
   - `GET /api/v1/streams/by-device/{device_id}` 的参数名是 `device_id`，实际也是 **device_tag 字符串**

3. **分页风格三套并存**：多数列表用 `limit`/`offset`，报警用 `page`/`page_size`，录像文件用 `page`/`limit`（且越界返回 400）。详见[分页与筛选约定](#分页与筛选约定)。

4. **Hook 端点响应格式不同**：返回 `{"code": 0, "msg": "ok"}`，`code` 为 `0` 且字段名是 `msg`，不能按统一封装解析。

5. **CORS 未启用**：后端未挂载 `CorsLayer`（虽然 `tower-http` 的 `cors` feature 已在依赖中）。浏览器跨域直连后端会被拦截，开发时依赖前端 Vite 代理（`/api`、`/ws` → `http://localhost:8081`）。

6. **请求链路追踪未生效**：`trace_layer` 计算了耗时与状态码但直接丢弃，`tower-http` 的 `TraceLayer` / `TimeoutLayer` 均未挂载。

7. **WebSocket 语音对讲未鉴权**：`/ws/audio-talk/...` 属于公开路由。

8. **未注册的处理器**：`src/api/http/handlers/` 下已实现但未挂载路由的接口包括设备级 PTZ 全套（`ptz_control`、`ptz_presets` 增删改查、`ptz_status`）、`GET /api/v1/devices/{id}/play-links`、`GET /api/v1/channels/by-device/{device_tag}` 等。

9. **前端字段名与后端不一致**：`PUT /api/v1/users/{id}/roles` 前端发送 `{roles}`，后端要求 `{role_ids: [uuid]}`；`PUT /api/v1/roles/{id}/permissions` 前端发送 `{permissions}`，后端要求 `{permission_ids: [uuid]}`。

10. **无 OpenAPI 描述文件**：仓库中没有 `utoipa` / `paperclip` / `swagger.json`，接口契约仅由 `src/api/http/routes.rs` 的路由表与本文档维护。新增或修改接口时请同步更新本文档。
