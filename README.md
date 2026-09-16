# RUSTCAM

现代化视频监控网关与信令控制平台，基于 Rust 后端 + Vue 前端构建，支持多协议设备接入、实时视频播放、录像回放、云台控制等功能。

## 功能特性

| 类别 | 功能 |
|------|------|
| 视频服务 | 实时播放、多画面布局、录像回放 |
| 设备管理 | 设备发现、注册、状态监控、分组管理 |
| 云台控制 | PTZ 协议支持、预置位管理 |
| 流媒体 | 集群部署、多服务器适配 (ZLMediKit/SRS/Xiu) |
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
| 状态 | Pinia |
| 图表 | ECharts + Vue-ECharts |
| 视频 | flv.js、hls.js、video.js |
| 构建 | Vite |

## 模块进度

| 模块 | 功能 | 状态 | 说明 |
|------|------|------|------|
| **GB28181** | SIP 信令 | ✅ 完成 | 支持 UDP/TCP 模式注册、INVITE、BYE |
| **GB28181** | 流接收 | ✅ 完成 | PS 封装解析，RTP over UDP/TCP |
| **GB28181** | 云台控制 | ✅ 完成 | 设备离线时返回 503，适配器自动注册 |
| **GB28181** | 语音对讲 | ✅ 完成 | 平台广播通知|
| **ONVIF** | 设备发现 | ✅ 完成 | WS-Discovery 广播探测 |
| **ONVIF** | 云台控制 | ✅ 完成 | ContinuousMove/AbsoluteMove/预置位 |
| **ONVIF** | 语音对讲 | 🔄 待开发 | 双向语音通信 |
| **RTSP** | 流接收 | ✅ 完成 | TCP/UDP 模式，支持 RTP over RTSP |
| **流媒体** | ZLMediaKit | ✅ 完成 | 集群管理、Hook 事件处理 |
| **流媒体** | SRS | 🔄 进行中 | 部分实现 |
| **流媒体** | XIU | 🔄 进行中 | 部分实现 |
| **前端** | 实时视频 | ✅ 完成 | flv.js/hls.js 多协议支持 |
| **前端** | PTZ 控制 | ✅ 完成 | D-pad + 摇杆 + 预置位独立卡片 |
| **前端** | 摄像头墙 | ✅ 完成 | 网格布局、拖拽排序 |
| **前端** | 录像回放 | ✅ 完成 | 时间轴检索、回放控制 |
| **认证** | JWT + RBAC | ✅ 完成 | 角色权限动态分配 |
| **传输** | TCP Server | ✅ 完成 | TCP传输模式 |
| **传输** | UDP Server | ✅ 完成 | UDP传输模式 |
| **传输** | TLS/SSL | 🔄 待开发 | HTTPS/WSS 支持 |

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
├── sql/                   # 数据库脚本
└── tests/                 # 测试用例
```

## 项目进度

- [x] GB28181 设备接入 (UDP/TCP)
- [x] ONVIF 设备发现与控制
- [x] RTSP 流接收
- [x] 云台 PTZ 控制 (GB28181 + ONVIF)
- [x] 前端 PTZ UI 重构 (控制面板 + 预置位分离)
- [x] 统一 API 响应格式 (中文错误消息)
- [ ] SRS 流媒体支持
- [ ] XIU 流媒体支持
- [ ] HTTPS/WSS 加密传输

## 截图预览

| 登录 | 仪表盘 | 设备分组 | 摄像头墙 |
|:----:|:------:|:--------:|:--------:|
| ![登录](./screenshot/1.login.png) | ![仪表盘](./screenshot/2.dashboard.png) | ![设备分组](./screenshot/3.device-group.png) | ![摄像头墙](./screenshot/4.cam-wall.png) |

## License

MIT License - 详见 [LICENSE](./LICENSE)
