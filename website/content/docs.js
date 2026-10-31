export default {
  zh: {
    meta: {
      title: '开发者文档 · RustCam',
      description: 'RustCam 的环境要求、目录结构、快速开始、配置说明、API 概览与常见问题。'
    },
    requirements: [
      { name: 'Rust', version: '1.70+' },
      { name: 'PostgreSQL', version: '14+' },
      { name: 'Redis', version: '6+' },
      { name: 'Node.js', version: '18+（前端构建）' }
    ],
    structure: {
      title: '项目结构',
      rows: [
        { path: 'src/', desc: 'Rust 后端：api / application / auth / domain / infrastructure / protocol / transport' },
        { path: 'frontend/', desc: 'Vue 3 控制台（Vite + Element Plus + Pinia）' },
        { path: 'clientapp/', desc: 'Qt 桌面客户端（CMake + FFmpeg）' },
        { path: 'public/', desc: '对外文档：api.md、architecture.mmd、architecture.png' },
        { path: 'sql/', desc: '数据库脚本（schema.sql）' },
        { path: 'tests/', desc: '测试用例' },
        { path: 'docs/', desc: '设计文档与实施计划' }
      ]
    },
    startup: {
      title: '快速开始',
      items: [
        { title: '初始化数据库', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: '启动后端', code: 'cargo run\n# 或发布构建\ncargo build --release\n./target/release/rustcam-media', lang: 'bash' },
        { title: '启动 Web 控制台', code: 'cd frontend\nnpm install\nnpm run dev', lang: 'bash' },
        { title: '登录并获取令牌', code: 'curl -s -X POST http://127.0.0.1:8081/api/v1/auth/login \\\n  -H \'Content-Type: application/json\' \\\n  -d \'{"username":"admin","password":"admin"}\'', lang: 'bash' }
      ]
    },
    config: {
      title: '配置文件',
      desc: '编辑仓库根目录的 config.toml。生产环境务必修改 JWT secret 与数据库口令。',
      code: '[server]\nhost = "0.0.0.0"\nport = 8081\ntls_enabled = false\n\n[database]\nurl = "postgres://user:pass@localhost:5432/rustcam"\nmax_connections = 10\n\n[redis]\nurl = "redis://localhost:6379"\npool_size = 5\n\n[[media_servers.servers]]\nname = "zlmediakit"\nurl = "http://127.0.0.1:80"\napi_key = "your_api_key"\nserver_type = "zlmediakit"   # zlmediakit / srs / xiu\nweight = 100\nenabled = true\nserver_tag = "default"\n\n[jwt]\nsecret = "change-me-in-production"'
    },
    api: {
      title: 'API 概览',
      desc: '后端基于 Axum 暴露 HTTP + WebSocket 接口，业务接口统一前缀 /api/v1，共 102 条受保护路由与 14 条公开路由。',
      authTitle: '认证方式',
      authDesc: '除公开端点外，所有接口都需在请求头携带 JWT：Authorization: Bearer <access_token>',
      groupsTitle: '分组速览',
      groups: [
        { group: '用户 / 角色 / 权限', count: 11, endpoints: 'GET/POST /users、PUT /users/{id}/roles、GET/POST /roles' },
        { group: '设备', count: 13, endpoints: 'GET/POST /devices、POST /devices/{id}/play、GET /devices/{id}/config' },
        { group: '通道', count: 7, endpoints: 'GET/POST /channels、GET /channels/{device_tag}/{channel_tag}/status' },
        { group: '云台 PTZ', count: 6, endpoints: 'POST /channels/{d}/{c}/ptz、GET/POST/PUT/DELETE .../ptz/presets' },
        { group: '流', count: 9, endpoints: 'GET/POST /streams、GET /streams/{id}/play、POST /streams/{id}/restart' },
        { group: '录像任务', count: 10, endpoints: 'GET/POST /recordings、POST /recordings/{id}/start|stop|pause|resume' },
        { group: '录像策略', count: 5, endpoints: 'GET/POST /recording-policies、PUT/DELETE /recording-policies/{id}' },
        { group: '媒体服务器', count: 10, endpoints: 'GET/POST /servers、GET /servers/{tag}/status、POST /servers/{tag}/enable' },
        { group: '监控墙布局', count: 6, endpoints: 'GET/POST /layouts、PUT /layouts/{id}/default' },
        { group: '设备分组 / 行政区划', count: 8, endpoints: 'GET /groups/tree、POST /groups、GET /regions/tree' },
        { group: 'ONVIF', count: 6, endpoints: 'POST /onvif/discover、POST /onvif/probe、POST /onvif/devices' },
        { group: '会话 / 告警 / 统计', count: 5, endpoints: 'GET /sessions、GET /alarms、GET /api/v1/stats' }
      ],
      publicTitle: '公开路由（14 条，无需认证）',
      publicRows: [
        { path: 'POST /api/v1/auth/login、/api/v1/auth/refresh', desc: '登录 / 刷新令牌' },
        { path: 'GET /api/v1/health、/api/v1/metrics、/api/v1/stats', desc: '健康 / 指标 / 统计' },
        { path: 'GET /api/v1/public/streams', desc: '公开直播' },
        { path: 'GET /api/v1/proxy/wfs', desc: '地图 WFS 代理' },
        { path: 'POST /hook/{event_name}、/hook/{server_type}/{event_name}', desc: '媒体服务器 Hook' },
        { path: 'POST /alarm/{stream_key}', desc: '外部告警上报' },
        { path: 'GET /ws、/ws/audio-talk/{device_tag}/{channel_id}', desc: '事件推送与语音对讲' }
      ]
    },
    faq: {
      title: '常见问题',
      items: [
        { q: '支持哪些设备接入方式？', a: 'GB28181（SIP 注册）、ONVIF（WS-Discovery 发现）、RTSP 直连以及 WebSocket 接入，统一抽象为设备与通道模型。' },
        { q: '云台控制设备离线时返回什么？', a: 'GB28181 设备离线时返回 503，适配器会自动重新注册设备。' },
        { q: '是否支持 HTTPS？', a: '支持。在 config.toml 中配置 tls_enabled、tls_cert 与 tls_key，由 rustls 实现 HTTPS 与 WSS。' },
        { q: '语音对讲如何实现？', a: '媒体侧通过 MediaNode 中间件链完成 G711 ⇄ PCM 转换与重采样，支撑 GB28181 广播与 ONVIF Backchannel 语音对讲。' },
        { q: '其他媒体服务器什么时候可用？', a: '媒体服务器适配层已抽象完毕，部分实现仍在推进，详见产品特性页的模块进度表。' }
      ]
    }
  },

  en: {
    meta: {
      title: 'Developer docs · RustCam',
      description: 'RustCam requirements, project layout, quick start, configuration, API overview and FAQ.'
    },
    requirements: [
      { name: 'Rust', version: '1.70+' },
      { name: 'PostgreSQL', version: '14+' },
      { name: 'Redis', version: '6+' },
      { name: 'Node.js', version: '18+ (frontend build)' }
    ],
    structure: {
      title: 'Project layout',
      rows: [
        { path: 'src/', desc: 'Rust backend: api / application / auth / domain / infrastructure / protocol / transport' },
        { path: 'frontend/', desc: 'Vue 3 console (Vite + Element Plus + Pinia)' },
        { path: 'clientapp/', desc: 'Qt desktop client (CMake + FFmpeg)' },
        { path: 'public/', desc: 'Public docs: api.md, architecture.mmd, architecture.png' },
        { path: 'sql/', desc: 'Database scripts (schema.sql)' },
        { path: 'tests/', desc: 'Test suites' },
        { path: 'docs/', desc: 'Design documents and implementation plans' }
      ]
    },
    startup: {
      title: 'Quick start',
      items: [
        { title: 'Initialise the database', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: 'Start the backend', code: 'cargo run\n# or a release build\ncargo build --release\n./target/release/rustcam-media', lang: 'bash' },
        { title: 'Start the web console', code: 'cd frontend\nnpm install\nnpm run dev', lang: 'bash' },
        { title: 'Log in and get a token', code: 'curl -s -X POST http://127.0.0.1:8081/api/v1/auth/login \\\n  -H \'Content-Type: application/json\' \\\n  -d \'{"username":"admin","password":"admin"}\'', lang: 'bash' }
      ]
    },
    config: {
      title: 'Configuration file',
      desc: 'Edit config.toml at the repository root. Always change the JWT secret and database password in production.',
      code: '[server]\nhost = "0.0.0.0"\nport = 8081\ntls_enabled = false\n\n[database]\nurl = "postgres://user:pass@localhost:5432/rustcam"\nmax_connections = 10\n\n[redis]\nurl = "redis://localhost:6379"\npool_size = 5\n\n[[media_servers.servers]]\nname = "zlmediakit"\nurl = "http://127.0.0.1:80"\napi_key = "your_api_key"\nserver_type = "zlmediakit"   # zlmediakit / srs / xiu\nweight = 100\nenabled = true\nserver_tag = "default"\n\n[jwt]\nsecret = "change-me-in-production"'
    },
    api: {
      title: 'API overview',
      desc: 'The Axum backend exposes HTTP + WebSocket endpoints under the /api/v1 prefix: 102 protected routes and 14 public routes.',
      authTitle: 'Authentication',
      authDesc: 'Every endpoint except the public ones requires a JWT in the request header: Authorization: Bearer <access_token>',
      groupsTitle: 'Endpoint groups',
      groups: [
        { group: 'Users / roles / permissions', count: 11, endpoints: 'GET/POST /users, PUT /users/{id}/roles, GET/POST /roles' },
        { group: 'Devices', count: 13, endpoints: 'GET/POST /devices, POST /devices/{id}/play, GET /devices/{id}/config' },
        { group: 'Channels', count: 7, endpoints: 'GET/POST /channels, GET /channels/{device_tag}/{channel_tag}/status' },
        { group: 'PTZ', count: 6, endpoints: 'POST /channels/{d}/{c}/ptz, GET/POST/PUT/DELETE .../ptz/presets' },
        { group: 'Streams', count: 9, endpoints: 'GET/POST /streams, GET /streams/{id}/play, POST /streams/{id}/restart' },
        { group: 'Recording tasks', count: 10, endpoints: 'GET/POST /recordings, POST /recordings/{id}/start|stop|pause|resume' },
        { group: 'Recording policies', count: 5, endpoints: 'GET/POST /recording-policies, PUT/DELETE /recording-policies/{id}' },
        { group: 'Media servers', count: 10, endpoints: 'GET/POST /servers, GET /servers/{tag}/status, POST /servers/{tag}/enable' },
        { group: 'Video wall layouts', count: 6, endpoints: 'GET/POST /layouts, PUT /layouts/{id}/default' },
        { group: 'Device groups / regions', count: 8, endpoints: 'GET /groups/tree, POST /groups, GET /regions/tree' },
        { group: 'ONVIF', count: 6, endpoints: 'POST /onvif/discover, POST /onvif/probe, POST /onvif/devices' },
        { group: 'Sessions / alarms / stats', count: 5, endpoints: 'GET /sessions, GET /alarms, GET /api/v1/stats' }
      ],
      publicTitle: 'Public routes (14, no authentication)',
      publicRows: [
        { path: 'POST /api/v1/auth/login, /auth/refresh', desc: 'Login / token refresh' },
        { path: 'GET /api/v1/health, /metrics, /stats', desc: 'Health / metrics / stats' },
        { path: 'GET /api/v1/public/streams', desc: 'Public live streams' },
        { path: 'GET /api/v1/proxy/wfs', desc: 'Map WFS proxy' },
        { path: 'POST /hook/{event_name}, /hook/{server_type}/{event_name}', desc: 'Media server hooks' },
        { path: 'POST /alarm/{stream_key}', desc: 'External alarm intake' },
        { path: 'GET /ws, /ws/audio-talk/{device_tag}/{channel_id}', desc: 'Event push and two-way talk' }
      ]
    },
    faq: {
      title: 'FAQ',
      items: [
        { q: 'How are devices onboarded?', a: 'Through GB28181 SIP registration, ONVIF WS-Discovery, direct RTSP or WebSocket — all unified into one device and channel model.' },
        { q: 'What happens when a PTZ device is offline?', a: 'GB28181 returns 503 and the adapter automatically re-registers the device.' },
        { q: 'Is HTTPS supported?', a: 'Yes. Set tls_enabled, tls_cert and tls_key in config.toml; HTTPS and WSS are implemented with rustls.' },
        { q: 'How does two-way talk work?', a: 'The MediaNode middleware chain performs G711 ⇄ PCM conversion and resampling, backing GB28181 broadcast and ONVIF Backchannel talk.' },
        { q: 'When will other media servers be available?', a: 'The media server adapter is ready; some integrations are still partially implemented — see the module progress table on the features page.' }
      ]
    }
  }
}
