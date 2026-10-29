# 贡献指南

感谢你关注 RustCam-Media（RustCam）。本文件说明如何在本仓库中搭建开发环境、运行测试，以及提交代码的约定。

项目主体语言为 **Rust（后端）+ Vue 3（Web 前端）+ C++/Qt（桌面客户端）**，仓库内文档以中文为主。

---

## 目录

- [贡献指南](#贡献指南)
  - [目录](#目录)
  - [开发环境](#开发环境)
    - [安装 PostgreSQL 的 PostGIS 扩展](#安装-postgresql-的-postgis-扩展)
    - [安装依赖](#安装依赖)
  - [本地启动](#本地启动)
    - [1. 初始化数据库](#1-初始化数据库)
    - [2. 修改 `config.toml`](#2-修改-configtoml)
    - [3. 启动后端](#3-启动后端)
    - [4. 启动前端](#4-启动前端)
    - [5. 运行桌面客户端](#5-运行桌面客户端)
  - [项目结构](#项目结构)
  - [代码规范](#代码规范)
    - [Rust](#rust)
    - [Vue 前端](#vue-前端)
    - [Qt 桌面客户端](#qt-桌面客户端)
  - [提交规范](#提交规范)
  - [分支规范](#分支规范)
  - [测试](#测试)
    - [后端](#后端)
    - [前端](#前端)
    - [桌面客户端](#桌面客户端)
  - [Pull Request 流程](#pull-request-流程)
  - [Issue 规范](#issue-规范)
  - [文档维护](#文档维护)
  - [行为准则](#行为准则)

---

## 开发环境

| 组件 | 版本要求 | 用途 |
|------|----------|------|
| Rust | 1.70+（edition 2021） | 后端服务 |
| Node.js | 18+ | 前端构建 / 开发服务器 / Playwright |
| PostgreSQL | 14+（需 **PostGIS** 扩展） | 业务持久化 |
| Redis | 6+ | 流信息缓存、媒体服务器心跳、事件 Pub/Sub |
| Qt | 5.15.2（推荐）或 Qt 6.2+ | 桌面客户端 |
| Visual Studio | 2019（MSVC v142） | 编译 Qt 客户端 |
| CMake | 3.16+ | 桌面客户端构建 |
| ZLMediaKit | 任意近期版本 | 流媒体服务器（可选，联调拉流 / 转推时需要） |

### 安装 PostgreSQL 的 PostGIS 扩展

电子地图的通道坐标依赖 PostGIS：

```bash
psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS postgis;"
```

### 安装依赖

```bash
# 后端
cargo build

# 前端
cd frontend && npm install

# 桌面客户端（Qt 路径按本机实际位置调整）
cd clientapp && cmake --preset vs2019-qt5152 && cmake --build build-vs2019 --config Release
```

---

## 本地启动

### 1. 初始化数据库

```bash
psql -U postgres -c "CREATE DATABASE rustcam;"
psql -U postgres -d rustcam -f sql/schema.sql
```

> `sql/schema.sql` 是唯一的建库脚本，仓库中 **没有** `sql/init.sql`。它会建出 `devices`、`channels`、`streams`、`sessions`、`servers`、`recordings`、`recording_policies`、`alarms`、`users` / `roles` / `permissions` 等全部业务表，并自动创建 PostGIS 几何触发器。
>
> 后续的结构变更以 **Python 迁移脚本** 的形式追加在 `sql/migrate_*.py`，在 `sql/` 目录下按需执行。

### 2. 修改 `config.toml`

```toml
[server]
host = "0.0.0.0"
port = 8081

[database]
url = "postgres://postgres:123456@127.0.0.1:5432/rustcam"

[redis]
url = "redis://127.0.0.1:6379/"

[jwt]
secret = "change-me-in-production"     # 生产环境务必修改
```

常用配置段：`[server]`、`[database]`、`[redis]`、`[media_servers]`、`[cluster]`、`[session]`、`[onvif]`、`[signaling_server]`（GB28181 SIP 端口）、`[rtsp_server]`、`[jwt]`、`[log]`。

> GB28181 联调时，`[signaling_server].bind_ip` / `rtp_bind_addr` 需要填本机在局域网中的实际 IP，否则设备无法回连 RTP 流。

### 3. 启动后端

```bash
cargo run              # 开发模式
cargo run --release    # 发布模式
```

服务默认监听 `0.0.0.0:8081`，可访问 `http://127.0.0.1:8081/api/v1/health` 验证。

### 4. 启动前端

```bash
cd frontend
npm run dev            # http://localhost:5173
```

Vite 开发服务器会把 `/api` 与 `/ws` 代理到 `http://localhost:8081`（见 `frontend/vite.config.js`），无需额外配置跨域。

生产构建：

```bash
npm run build          # 产物在 frontend/dist
```

### 5. 运行桌面客户端

在 Qt Creator 中打开 `clientapp/CMakeLists.txt`，或使用命令行：

```bash
cd clientapp
cmake --preset vs2019-qt5152
cmake --build build-vs2019 --config Release
./build-vs2019/bin/Release/rustcam-client.exe
```

客户端默认连接 `http://localhost:8080`，可在 `设置 → 服务器配置` 中改为后端实际地址。

---

## 项目结构

```
RustCam-Media/
├── src/                    # Rust 后端
│   ├── api/                # HTTP 层：路由、处理器、响应封装、WebSocket
│   ├── application/        # 应用服务层：设备、流、录像、媒体服务器、Hook
│   ├── domain/             # 领域模型与端口（traits）
│   ├── protocol/           # 协议适配：GB28181 / ONVIF / RTSP / WebSocket
│   ├── transport/          # SIP 信令服务端（TCP / UDP）
│   ├── media/              # 媒体数据面：RTP 服务、MediaNode 中间件链
│   ├── adapter/            # 媒体服务器适配（ZLMediaKit / SRS / XIU）
│   ├── infrastructure/     # 持久化与缓存实现（Rbatis / Redis / 集群）
│   ├── auth/               # 认证授权：JWT、Casbin、用户与角色
│   ├── context/            # 依赖注入容器（ServiceRegistry）
│   └── monitoring/         # 指标
├── frontend/               # Vue 3 Web 控制台（Vite + Element Plus + Pinia）
├── clientapp/              # Qt5/C++ 桌面客户端（CMake + FFmpeg + CEF）
├── sql/                    # PostgreSQL 建库脚本与迁移脚本
├── public/                 # 对外文档（API 参考、架构图及其源码）
├── conf/                   # ZLMediaKit 配置模板
├── tests/                  # Rust 集成测试
└── docs/                   # 内部设计与实施计划（不纳入版本库）
```

分层约定：**HTTP 处理器 → 应用服务 → 领域模型 / 端口 → 基础设施实现**。新增能力时应遵循这一方向，避免在处理器里直接写 SQL 或直接操作 Redis。

---

## 代码规范

### Rust

- 统一使用 `rustfmt` 默认风格，提交前执行：

  ```bash
  cargo fmt
  cargo clippy --all-targets -- -D warnings
  ```

- 错误统一使用 `thiserror` 定义的 `AppError` / `AuthError`，由 `into_response` 转换为统一响应封装。
- 业务错误信息使用中文，编码为 UTF-8（MSVC 下需 `/utf-8`，`Cargo.toml` 已通过 profile 处理）。
- 共享状态优先使用 `DashMap` / `parking_lot::RwLock`，避免无谓的 `Mutex` 竞争。
- 单元测试与实现同文件（`#[cfg(test)] mod tests`），集成测试放在 `tests/`。

### Vue 前端

- 组件使用 `<script setup>` 组合式 API，Element Plus 组件优先于手写样式。
- 状态集中在 `frontend/src/stores/`，接口调用集中在 `frontend/src/api/`，组件不直接写 URL。
- 请求统一走 `frontend/src/utils/request.js` 的 axios 实例，由拦截器自动附加 `Authorization` 头并在 401 时自动刷新令牌。
- 播放组件统一使用 `frontend/src/components/VideoPlayer.vue`，按协议自动选择 HLS / FLV / WebRTC。

### Qt 桌面客户端

详见 `clientapp/document/framework.md`，要点：

- 界面优先使用 `.ui` 文件配合 Qt Designer，通过 `uic` 生成代码；仅在动态逻辑处用代码补充。
- 禁止在代码中硬编码中文字符串，一律使用 `tr()` 封装。
- 禁止硬编码颜色，使用主题变量或样式表。
- 所有用户界面控件必须设置 `objectName`，便于样式与自动化测试定位。
- 使用 Qt5 新式信号槽语法 `connect(sender, &Sender::signal, receiver, &Receiver::slot)`。
- 新功能尽量以插件形式加入 `clientapp/rustcam_plugins/`。

---

## 提交规范

示例：

```
frontend: 通道编辑弹窗接入地图选点
device: 修复设备按 tag 更新时坐标未迁移的问题
protocol: 抽取 GB28181 SIP 解析为独立模块
api: 补充录像文件分页说明
```

---

## 分支规范

- 主干分支为 `master`，始终保持可构建、可运行。
- 从 `master` 切出功能分支，完成后以 squash merge 或 fast-forward 方式合回。

---

## 测试

提交 PR 前请至少跑通与改动相关的测试。

### 后端

```bash
cargo test                       # 单元测试 + 集成测试
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check
```

### 前端

```bash
cd frontend
npm run build                     # 构建校验
npm run test                      # Playwright 端到端测试
npm run test:core                 # 只跑录像相关核心用例
```

> Playwright 用例依赖本地后端与数据库，请先启动后端再执行。测试脚本位于 `frontend/tests/`，默认以 `http://localhost:5173` 为 baseURL，可用 `BASE_URL` 环境变量覆盖。

### 桌面客户端

```bash
cd clientapp
cmake --preset vs2019-qt5152     # 预设中已开启 BUILD_TESTS
cmake --build build-vs2019 --config Debug
ctest --test-dir build-vs2019 -C Debug --output-on-failure
```

---

## Pull Request 流程

1. **先开 Issue**：较大的功能或改动请先建 Issue 讨论方案，避免白做。
2. **切分支**：从最新的 `master` 切出功能分支。
3. **提交**：保持小步提交，每次提交只做一件事。
4. **自测**：本地跑通相关测试，更新受影响的文档。
5. **提 PR**：说明改动动机、影响范围、验证方式，并关联 Issue。
6. **评审**：至少一位维护者评审通过后合入 `master`。
7. **合入后**：如涉及接口变更，同步更新 `public/api.md`；如涉及架构调整，同步更新 `public/architecture.mmd` 并重新生成 PNG。

---

## Issue 规范

标题格式：

```
[模块] 问题简述
```

模块取值参考：`backend` / `frontend` / `clientapp` / `device` / `channel` / `stream` / `recording` / `protocol` / `media-server` / `db` / `doc` / `other`。

正文建议包含：

- 复现步骤
- 期望结果 vs 实际结果
- 相关日志（`logs/rustcam.log.*`，开启 `[log].sip = true` 可获得完整 SIP 报文）
- 版本信息（后端 commit、前端构建时间、Qt 客户端版本）

涉及 GB28181 / ONVIF 的问题，附上抓包（`*.pcapng`）会显著加快定位速度。

---

## 文档维护

| 文件 | 内容 | 更新时机 |
|------|------|----------|
| `README.md` | 项目总览、截图、部署说明 | 功能、部署方式变化时 |
| `public/api.md` | 完整 API 参考（102 受保护 + 14 公开路由） | **任何接口新增 / 修改 / 删除都必须同步更新** |
| `public/architecture.mmd` | 架构图源码（Mermaid） | 架构分层或依赖关系变化时 |
| `public/architecture.png` | 架构图成品 | 修改 `.mmd` 后重新生成 |
| `CONTRIBUTING.md` | 本文件 | 开发流程变化时 |

重新生成架构图（需要 Node.js 与 Playwright，Mermaid 库首次运行会从 CDN 拉取并缓存）：

```bash
node public/render-architecture.mjs
```

> 注意：Mermaid 的子图标题固定按 200px 宽度排版，过长会被截断，修改 `public/architecture.mmd` 时请把子图标题控制在 12 个汉字以内。

---

## 行为准则

提交代码即表示你同意本项目采用 **MIT 协议**（见 `LICENSE`）。请确保提交的代码符合该协议，并尊重其他贡献者的劳动成果。
