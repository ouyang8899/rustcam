export default {
  zh: {
    meta: {
      title: 'RustCam · Rust 驱动的视频监控网关与信令控制平台',
      description: 'RustCam 是基于 Rust 构建的现代化视频监控网关，支持 GB28181、ONVIF、RTSP 多协议设备接入，实时视频、录像回放、云台控制、电子地图与流媒体集群。'
    },
    hero: {
      eyebrow: 'RUST + VUE + QT',
      title: 'Rust 驱动的\n摄像头视频监控网关',
      subtitle: '多协议设备接入、实时音视频、录像回放与信令控制，一套平台覆盖 Web 控制台、桌面客户端与信令服务。',
      primaryCta: '查看开发者文档',
      secondaryCta: 'Gitee 仓库',
      stats: [
        { value: 102, suffix: ' 条', label: '受保护 API 路由' },
        { value: 4, suffix: ' 大', label: '协议族接入' },
        { value: 8, suffix: ' 大', label: '能力域' },
        { value: 3, suffix: ' 端', label: 'Web / 桌面 / 服务端' }
      ]
    },
    protocols: {
      title: '协议与媒体栈',
      description: '信令侧通过统一适配器抽象，视频侧对接成熟的媒体服务器。',
      items: [
        { name: 'GB28181', desc: 'SIP 信令注册、INVITE 呼叫、PS 流解析、平台广播与语音对讲' },
        { name: 'ONVIF', desc: 'WS-Discovery 设备发现、ContinuousMove/AbsoluteMove 云台、Backchannel 对讲' },
        { name: 'RTSP', desc: 'TCP / UDP 模式接入，支持 RTP over RTSP 直连拉流' },
        { name: 'WebSocket', desc: '事件实时推送与语音对讲双向通道，基于 JWT 鉴权' }
      ]
    },
    features: {
      eyebrow: '核心能力',
      title: '监控平台该有的，都在这里',
      description: '从设备接入到媒体分发，从录像回放到电子地图，一套后端对外提供统一 API。',
      items: [
        { icon: 'camera', title: '实时视频', description: 'flv.js / hls.js / video.js 多协议播放，支持单画面与多画面布局。', items: ['Web 控制台多画面', '多协议自适应播放', '实时会话状态可见'] },
        { icon: 'broadcast', title: '多协议接入', description: 'GB28181、ONVIF、RTSP、WebSocket 统一抽象为设备与通道模型。', items: ['设备发现与注册', '分组与行政区划', '通道级状态监控'] },
        { icon: 'route', title: '云台 PTZ', description: '方向控制、变倍、预置位管理，设备离线时适配器自动重注册。', items: ['D-pad 与摇杆控制', '预置位独立管理', 'GB28181 + ONVIF 双协议'] },
        { icon: 'map-pin', title: '电子地图', description: 'MapLibre GL JS 渲染 GeoServer 图层，设备点位聚合与状态筛选。', items: ['点位聚合渲染', '地图选点绑定坐标', '点位状态筛选'] },
        { icon: 'clock', title: '录像与回放', description: '录像任务、录像策略、文件管理与多通道回放时间轴检索。', items: ['录像策略调度', '多通道并行回放', '回放进度精确控制'] },
        { icon: 'server', title: '流媒体集群', description: '按权重调度与故障切换。', items: ['多服务器权重路由', 'Hook 事件处理', '启停与状态探测'] }
      ]
    },
    architecture: {
      eyebrow: '系统架构',
      title: '清晰的分层，干净的抽象',
      description: 'Axum + Tokio 分层架构：HTTP 处理器 → 应用服务 → 领域模型/端口 → 基础设施实现。协议侧由 SignalAdapter 统一抽象，媒体侧由 MediaServerAdapter 对接媒体服务器。',
      points: [
        { title: 'SignalAdapter 统一协议抽象', desc: 'GB28181 / ONVIF / RTSP / WebSocket 以同一端口接入，新增协议不改上层业务。' },
        { title: 'MediaNode 中间件链', desc: '承担 G711 ⇄ PCM 转换与重采样，支撑 GB28181 广播与 ONVIF Backchannel 语音对讲。' },
        { title: 'MediaServerAdapter 媒体适配', desc: '同一套媒体服务器接口对接不同媒体服务器，集群化部署无需改业务代码。' }
      ]
    },
    gallery: {
      eyebrow: '界面预览',
      title: 'Web 控制台与桌面客户端',
      description: '以下均为项目真实运行界面截图，可点击放大。',
      web: {
        title: 'Web 控制台',
        items: [
          { src: '/img/console-overview.webp', alt: '系统概览面板', caption: '系统概览', width: 1920, height: 1082 },
          { src: '/img/console-devices.webp', alt: '设备列表', caption: '设备列表', width: 1920, height: 1082 },
          { src: '/img/console-channels.webp', alt: '通道列表', caption: '通道列表', width: 1920, height: 1082 },
          { src: '/img/console-map-points.webp', alt: '电子地图设备点位', caption: '电子地图 · 设备点位', width: 1920, height: 1082 },
          { src: '/img/console-tvwall.webp', alt: '监控墙', caption: '监控墙', width: 1920, height: 1082 },
          { src: '/img/console-playback.webp', alt: '录像回放', caption: '录像回放', width: 1920, height: 1082 }
        ]
      },
      desktop: {
        title: 'Qt 桌面客户端',
        items: [
          { src: '/img/desktop-live.webp', alt: '客户端实时预览', caption: '客户端 · 实时预览', width: 1920, height: 1041 },
          { src: '/img/desktop-map.webp', alt: '客户端电子地图', caption: '客户端 · 电子地图', width: 1920, height: 1041 },
          { src: '/img/desktop-playback.webp', alt: '客户端录像回放', caption: '客户端 · 录像回放', width: 1920, height: 1041 }
        ]
      }
    },
    scenarios: {
      eyebrow: '应用场景',
      title: '从园区到城市',
      description: '把分散的摄像机、编码器与报警设备收敛到一个可运营的平台。',
      items: [
        { icon: 'grid', title: '园区安防', desc: '统一纳管园区内不同品牌摄像机，电子地图 + 监控墙快速定位事件。' },
        { icon: 'globe', title: '城市治理', desc: '通过 GB28181 汇聚区域级视频资源，支撑城市级运营中心。' },
        { icon: 'cpu', title: '工业安防', desc: '生产现场设备与办公区域权限隔离，报警记录闭环处理。' },
        { icon: 'route', title: '交通枢纽', desc: '高并发实时通道接入，录像策略按时间段自动执行。' }
      ]
    },
    quickStart: {
      eyebrow: '快速开始',
      title: '三步跑起来',
      description: '克隆仓库、准备依赖、启动服务。',
      steps: [
        { title: '克隆仓库', code: 'git clone https://gitee.com/ouliuying/rustcam.git\ncd rustcam', lang: 'bash' },
        { title: '准备数据库与缓存', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: '构建并启动', code: 'cargo build --release\n./target/release/rustcam-media', lang: 'bash' }
      ],
      frontendTitle: '启动 Web 控制台',
      frontendCode: 'cd frontend\nnpm install\nnpm run dev',
      note: '桌面客户端使用 CMake 构建 Qt 5/6 工程；完整说明见开发者文档。'
    },
    techStack: {
      eyebrow: '技术栈',
      title: '为长期维护而选',
      description: '每一层都选了成熟、可替换、有人维护的技术。',
      columns: [
        { title: '后端 (Rust)', items: ['Axum + Tokio 分层架构', 'RTSP / GB28181 / ONVIF / WebSocket', 'JWT + Casbin 鉴权', 'Redis 缓存 + PostgreSQL 持久化'] },
        { title: '前端 (Vue)', items: ['Vue 3 + Element Plus', 'Pinia 状态管理', 'ECharts 图表', 'flv.js / hls.js / video.js'] },
        { title: '桌面端 (Qt)', items: ['Qt 5 / 6', 'CMake 构建', 'FFmpeg 音视频', '跨平台桌面客户端'] }
      ]
    },
    cta: {
      title: '把监控接入这件事交给 Rust',
      description: '开源、MIT 协议、文档齐全。克隆源码，或先读开发者文档了解 API 与部署方式。',
      primary: '查看开发者文档',
      secondary: 'Gitee 仓库'
    }
  },

  en: {
    meta: {
      title: 'RustCam · A Camera video surveillance gateway written in Rust',
      description: 'RustCam is a modern video surveillance gateway and signalling platform built with Rust, supporting GB28181, ONVIF and RTSP ingestion, live playback, recording, PTZ control and an electronic map.'
    },
    hero: {
      eyebrow: 'RUST + VUE + QT',
      title: 'A video surveillance\ngateway written in Rust',
      subtitle: 'Multi-protocol ingestion, live media, recording playback and signalling control — one platform for the web console, the desktop client and the signalling server.',
      primaryCta: 'Read the docs',
      secondaryCta: 'Gitee repository',
      stats: [
        { value: 102, suffix: '', label: 'protected API routes' },
        { value: 4, suffix: '', label: 'protocol families' },
        { value: 8, suffix: '', label: 'capability domains' },
        { value: 3, suffix: '', label: 'web / desktop / server' }
      ]
    },
    protocols: {
      title: 'Protocols & media stack',
      description: 'Signalling is unified behind one adapter; video is served by mature streaming servers.',
      items: [
        { name: 'GB28181', desc: 'SIP registration, INVITE calls, PS stream parsing, platform broadcast and two-way talk' },
        { name: 'ONVIF', desc: 'WS-Discovery discovery, ContinuousMove/AbsoluteMove PTZ, Backchannel audio' },
        { name: 'RTSP', desc: 'TCP / UDP modes, including direct RTP over RTSP pull' },
        { name: 'WebSocket', desc: 'Real-time event push and two-way talk channel, JWT protected' }
      ]
    },
    features: {
      eyebrow: 'Core capabilities',
      title: 'Everything a surveillance platform needs',
      description: 'From device ingestion to media distribution, from recording playback to the electronic map — one backend, one unified API.',
      items: [
        { icon: 'camera', title: 'Live video', description: 'flv.js / hls.js / video.js playback with single and multi-view layouts.', items: ['Multi-view console', 'Protocol-adaptive playback', 'Visible session state'] },
        { icon: 'broadcast', title: 'Multi-protocol ingestion', description: 'GB28181, ONVIF, RTSP and WebSocket unified into one device and channel model.', items: ['Discovery and registration', 'Groups and admin regions', 'Per-channel status'] },
        { icon: 'route', title: 'PTZ control', description: 'Pan, tilt, zoom and preset management with automatic adapter re-registration when a device is offline.', items: ['D-pad and joystick', 'Dedicated preset panel', 'GB28181 + ONVIF'] },
        { icon: 'map-pin', title: 'Electronic map', description: 'MapLibre GL JS over GeoServer layers, with point clustering and status filters.', items: ['Clustered markers', 'Pick coordinates on the map', 'Status filtering'] },
        { icon: 'clock', title: 'Recording & playback', description: 'Recording tasks, policies, file management and multi-channel timeline playback.', items: ['Policy scheduling', 'Parallel multi-channel playback', 'Frame-accurate seek'] },
        { icon: 'server', title: 'Streaming cluster', description: 'Media Server behind one adapter, with weighted routing and failover.', items: ['Weighted server routing', 'Hook event handling', 'Enable / disable and probing'] }
      ]
    },
    architecture: {
      eyebrow: 'Architecture',
      title: 'Clear layers, clean abstractions',
      description: 'Axum + Tokio layering: HTTP handlers → application services → domain model and ports → infrastructure. Signalling goes through SignalAdapter, media through MediaServerAdapter.',
      points: [
        { title: 'SignalAdapter', desc: 'GB28181 / ONVIF / RTSP / WebSocket share a single port; adding a protocol does not touch the business layer.' },
        { title: 'MediaNode middleware chain', desc: 'Handles G711 ⇄ PCM conversion and resampling for GB28181 broadcast and ONVIF Backchannel talk.' },
        { title: 'MediaServerAdapter', desc: 'One media server interface targets any media server, so clustered deployment needs no business changes.' }
      ]
    },
    gallery: {
      eyebrow: 'Interface preview',
      title: 'Web console and desktop client',
      description: 'Every screenshot below comes from a real running instance — click to zoom.',
      web: {
        title: 'Web console',
        items: [
          { src: '/img/console-overview.webp', alt: 'System overview', caption: 'System overview', width: 1920, height: 1082 },
          { src: '/img/console-devices.webp', alt: 'Device list', caption: 'Device list', width: 1920, height: 1082 },
          { src: '/img/console-channels.webp', alt: 'Channel list', caption: 'Channel list', width: 1920, height: 1082 },
          { src: '/img/console-map-points.webp', alt: 'Electronic map markers', caption: 'Electronic map · markers', width: 1920, height: 1082 },
          { src: '/img/console-tvwall.webp', alt: 'Video wall', caption: 'Video wall', width: 1920, height: 1082 },
          { src: '/img/console-playback.webp', alt: 'Recording playback', caption: 'Recording playback', width: 1920, height: 1082 }
        ]
      },
      desktop: {
        title: 'Qt desktop client',
        items: [
          { src: '/img/desktop-live.webp', alt: 'Desktop live preview', caption: 'Desktop · live preview', width: 1920, height: 1041 },
          { src: '/img/desktop-map.webp', alt: 'Desktop electronic map', caption: 'Desktop · electronic map', width: 1920, height: 1041 },
          { src: '/img/desktop-playback.webp', alt: 'Desktop recording playback', caption: 'Desktop · recording playback', width: 1920, height: 1041 }
        ]
      }
    },
    scenarios: {
      eyebrow: 'Scenarios',
      title: 'From a campus to a city',
      description: 'Pull scattered cameras, encoders and alarm devices into one operable platform.',
      items: [
        { icon: 'grid', title: 'Campus security', desc: 'Onboard cameras from any vendor, locate incidents fast with the map and video wall.' },
        { icon: 'globe', title: 'City operations', desc: 'Aggregate regional video resources over GB28181 for a city-level operation centre.' },
        { icon: 'cpu', title: 'Industrial safety', desc: 'Separate permissions between production floors and offices, close the loop on alarms.' },
        { icon: 'route', title: 'Transport hubs', desc: 'High-concurrency live channels with recording policies driven by time schedules.' }
      ]
    },
    quickStart: {
      eyebrow: 'Quick start',
      title: 'Up in three steps',
      description: 'Clone the repository, prepare dependencies, start the service.',
      steps: [
        { title: 'Clone the repository', code: 'git clone https://gitee.com/ouliuying/rustcam.git\ncd rustcam', lang: 'bash' },
        { title: 'Prepare database and cache', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: 'Build and run', code: 'cargo build --release\n./target/release/rustcam-media', lang: 'bash' }
      ],
      frontendTitle: 'Start the web console',
      frontendCode: 'cd frontend\nnpm install\nnpm run dev',
      note: 'The desktop client is a Qt 5/6 CMake project. See the developer docs for the full setup.'
    },
    techStack: {
      eyebrow: 'Tech stack',
      title: 'Chosen for long-term maintenance',
      description: 'Every layer uses a mature, replaceable, well-maintained technology.',
      columns: [
        { title: 'Backend (Rust)', items: ['Axum + Tokio layered architecture', 'RTSP / GB28181 / ONVIF / WebSocket', 'JWT + Casbin authorisation', 'Redis cache + PostgreSQL storage'] },
        { title: 'Frontend (Vue)', items: ['Vue 3 + Element Plus', 'Pinia state management', 'ECharts visualisation', 'flv.js / hls.js / video.js'] },
        { title: 'Desktop (Qt)', items: ['Qt 5 / 6', 'CMake build', 'FFmpeg media', 'Cross-platform client'] }
      ]
    },
    cta: {
      title: 'Let Rust handle device onboarding',
      description: 'Open source, MIT licensed, fully documented. Clone the source, or read the developer docs first.',
      primary: 'Read the developer docs',
      secondary: 'Gitee repository'
    }
  }
}
