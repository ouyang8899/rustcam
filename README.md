# RUSTCAM

RUSTCAM 是一个基于 Rust 后端和 Vue 前端打造的现代化视频监控网关与信令控制平台，支持多协议设备接入、实时视频播放录像回放、云台控制等功能。

## 技术架构

### 后端 (Rust)

基于 Rust 语言开发的高性能流媒体服务网关，专注于效率与安全：

- **多协议支持**: RTSP、GB28181、ONVIF、WebSocket
- **设备管理**: 设备发现、注册、状态监控
- **流媒体服务**: 支持 ZLMediKit、SRS、Xiu 等主流流媒体服务器适配
- **云台控制**: PTZ 协议支持与精准控制
- **用户认证**: JWT 认证 + Casbin 权限管理
- **数据存储**: Redis 缓存 + PostgreSQL 持久化

### 前端 (Vue)

现代化响应式 Web 管理界面：

- **框架**: Vue 3 + Element Plus
- **状态管理**: Pinia
- **图表可视化**: ECharts + Vue-ECharts
- **视频播放**: flv.js、hls.js、video.js 多格式支持
- **路由管理**: Vue Router

## 功能特性

- 视频实时播放与多画面布局
- 设备录像管理与回放
- 云台 PTZ 控制
- 设备分组与区域管理
- 用户权限控制
- 流媒体服务集群

## 项目结构

```
RUSTCAM/
├── src/                    # Rust 后端源码
│   ├── api/               # HTTP API 接口
│   ├── application/       # 应用服务层
│   ├── auth/              # 认证授权
│   ├── domain/            # 领域模型
│   ├── infrastructure/    # 基础设施层
│   ├── protocol/          # 协议实现 (RTSP/GB28181/ONVIF)
│   └── transport/         # 网络传输层
├── frontend/              # Vue 前端
│   └── src/               # 前端源码
├── sql/                   # 数据库脚本
└── tests/                 # 测试用例
```

## License

本项目采用 MIT 协议开源，详见 [LICENSE](./LICENSE) 文件。
