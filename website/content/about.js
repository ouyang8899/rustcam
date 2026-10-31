export default {
  zh: {
    meta: {
      title: '关于 · RustCam',
      description: 'RustCam 的项目定位、架构理念、路线图、贡献指南与开源许可。'
    },
    intro: {
      title: '关于 RustCam',
      desc: 'RustCam-Media 是一个现代化的视频监控网关与信令控制平台：Rust 后端负责协议接入、媒体转发与信令控制，Vue 前端提供 Web 控制台，Qt 客户端面向桌面运维场景。项目以 MIT 协议开源。'
    },
    principles: {
      title: '架构理念',
      items: [
        { title: '分层而非堆砌', desc: 'HTTP 处理器 → 应用服务 → 领域模型与端口 → 基础设施实现，业务规则不依赖具体协议或存储。' },
        { title: '端口与适配器', desc: '协议侧由 SignalAdapter 统一抽象 GB28181 / ONVIF / RTSP / WebSocket，媒体侧由 MediaServerAdapter 对接媒体服务器。' },
        { title: '媒体与信令分离', desc: '传输与媒体数据面（transport / media）负责 SIP 信令与 RTP 收发，通过 MediaNode 中间件链完成音频格式转换与重采样。' },
        { title: '安全默认开启', desc: 'JWT + Casbin 贯穿全部业务接口，TLS 由 rustls 实现，HTTPS 与 WSS 开箱可用。' }
      ]
    },
    roadmap: {
      title: '路线图',
      items: [
        { status: 'done', title: 'GB28181 全链路', desc: 'SIP 信令、PS 流解析、云台控制、平台广播与语音对讲均已完成。' },
        { status: 'done', title: 'ONVIF 发现与控制', desc: 'WS-Discovery 探测、ContinuousMove / AbsoluteMove 与 Backchannel 对讲已完成。' },
        { status: 'done', title: 'RTSP 与媒体服务器', desc: 'RTSP 接入与媒体服务器集群已完成，Hook 事件链路打通。' },
        { status: 'progress', title: '媒体服务器适配', desc: '媒体服务器适配层已抽象完毕，部分实现仍在推进中。' },
        { status: 'next', title: '能力持续演进', desc: '监控墙布局、录像策略、地图选点等能力持续迭代。' }
      ]
    },
    contribute: {
      title: '参与贡献',
      desc: '开发环境、本地启动、代码规范、提交规范与 PR 流程详见仓库中的 CONTRIBUTING.md。提交代码前请至少执行以下命令：',
      code: 'cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test',
      note: '提交信息采用 Conventional Commits 规范，例如 feat(frontend): 通道编辑弹窗接入地图选点。'
    },
    license: {
      title: '开源许可',
      desc: 'RustCam-Media 基于 MIT License 发布，允许商业使用、修改、分发与再授权，保留版权声明即可。'
    }
  },

  en: {
    meta: {
      title: 'About · RustCam',
      description: 'RustCam project positioning, architecture principles, roadmap, contribution guide and open-source licence.'
    },
    intro: {
      title: 'About RustCam',
      desc: 'RustCam-Media is a modern video surveillance gateway and signalling control platform: a Rust backend handling protocol ingestion, media forwarding and signalling control, a Vue web console, and a Qt client for desktop operations. Released under the MIT licence.'
    },
    principles: {
      title: 'Architecture principles',
      items: [
        { title: 'Layers, not a pile', desc: 'HTTP handlers → application services → domain model and ports → infrastructure; business rules never depend on a protocol or a database.' },
        { title: 'Ports and adapters', desc: 'SignalAdapter unifies GB28181 / ONVIF / RTSP / WebSocket; MediaServerAdapter targets media servers.' },
        { title: 'Media separated from signalling', desc: 'The transport and media planes handle SIP signalling and RTP, with a MediaNode middleware chain performing audio conversion and resampling.' },
        { title: 'Secure by default', desc: 'JWT + Casbin guard every business endpoint, TLS is implemented with rustls, so HTTPS and WSS work out of the box.' }
      ]
    },
    roadmap: {
      title: 'Roadmap',
      items: [
        { status: 'done', title: 'Full GB28181 chain', desc: 'SIP signalling, PS stream parsing, PTZ, platform broadcast and two-way talk are all complete.' },
        { status: 'done', title: 'ONVIF discovery and control', desc: 'WS-Discovery probing, ContinuousMove / AbsoluteMove and Backchannel talk are complete.' },
        { status: 'done', title: 'RTSP and media servers', desc: 'RTSP ingestion and the media server cluster are complete, with the hook event chain wired up.' },
        { status: 'progress', title: 'Media server adapters', desc: 'The adapter layer is finished; some integrations are still partially implemented.' },
        { status: 'next', title: 'Continuous evolution', desc: 'Video wall layouts, recording policies and map pickers keep iterating.' }
      ]
    },
    contribute: {
      title: 'Contribute',
      desc: 'The development environment, local startup, code style, commit conventions and the PR flow are described in CONTRIBUTING.md. Before submitting, run at least:',
      code: 'cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test',
      note: 'Commit messages follow Conventional Commits, for example: feat(frontend): 通道编辑弹窗接入地图选点。'
    },
    license: {
      title: 'Open-source licence',
      desc: 'RustCam-Media is released under the MIT License: commercial use, modification, distribution and sublicensing are allowed as long as the copyright notice is retained.'
    }
  }
}
