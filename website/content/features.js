export default {
  zh: {
    meta: {
      title: '产品特性 · RustCam',
      description: 'RustCam 的完整能力清单：实时视频、设备接入、云台控制、流媒体集群、电子地图、录像回放、告警与权限。'
    },
    domains: [
      { id: 'video', icon: 'camera', title: '视频服务', summary: '实时播放、多画面布局、录像回放', items: ['flv.js / hls.js / video.js 多协议播放', '单画面与多画面布局切换', '录像回放与时间轴检索', '实时会话状态可见'] },
      { id: 'device', icon: 'broadcast', title: '设备管理', summary: '发现、注册、状态监控与分组', items: ['GB28181 SIP 注册与设备接入', 'ONVIF WS-Discovery 设备发现', '设备分组与行政区划', '通道级在线状态监控'] },
      { id: 'ptz', icon: 'route', title: '云台控制', summary: 'PTZ 协议支持与预置位管理', items: ['GB28181 与 ONVIF 双协议控制', 'ContinuousMove / AbsoluteMove', '预置位增删改查', '设备离线返回 503 并自动重注册'] },
      { id: 'stream', icon: 'server', title: '媒体服务器', summary: '集群部署与统一适配', items: ['主流媒体服务器完整支持', '多厂商媒体服务器适配', '按权重调度与启停控制', 'Hook 事件回调处理'] },
      { id: 'map', icon: 'map-pin', title: '电子地图', summary: '地图渲染、设备标记与状态筛选', items: ['MapLibre GL JS 渲染', 'GeoServer 图层与 WFS 代理', '设备点位聚合', '地图选点绑定通道坐标'] },
      { id: 'record', icon: 'clock', title: '录像与回放', summary: '录像任务、策略与文件管理', items: ['录像任务启停、暂停与恢复', '按时间段录像策略', '录像文件与播放地址', '多通道并行回放'] },
      { id: 'alarm', icon: 'bell', title: '告警', summary: '告警上报、记录与处理闭环', items: ['外部告警上报接口', '告警记录查询与分页', '处理状态标记', '告警与会话联动'] },
      { id: 'auth', icon: 'lock', title: '用户与权限', summary: 'JWT 认证与 RBAC 权限控制', items: ['JWT 登录与令牌刷新', 'Casbin 策略授权', '用户 / 角色 / 权限三级模型', '动态角色权限分配'] }
    ],
    protocols: {
      title: '协议兼容',
      columns: ['协议', '状态', '能力'],
      rows: [
        { name: 'GB28181', status: '完成', support: 'SIP 信令、流接收、PTZ、平台广播、语音对讲' },
        { name: 'ONVIF', status: '完成', support: '设备发现、PTZ 控制、Backchannel 语音对讲' },
        { name: 'RTSP', status: '完成', support: 'TCP / UDP 模式，RTP over RTSP' },
        { name: 'WebSocket', status: '完成', support: '事件推送、语音对讲双向通道' },
      ]
    },
    progress: {
      title: '模块进度',
      columns: ['模块', '状态', '说明'],
      rows: [
        { module: 'GB28181 信令 / 流 / PTZ / 对讲', status: '完成', note: 'UDP 与 TCP 模式均支持' },
        { module: 'ONVIF 发现 / PTZ / 对讲', status: '完成', note: '广播探测与自动注册' },
        { module: 'RTSP 流接收', status: '完成', note: '支持 RTP over RTSP' },
        { module: '前端实时视频 / PTZ / 监控墙', status: '完成', note: 'D-pad + 摇杆 + 预置位独立卡片' },
        { module: '电子地图', status: '完成', note: 'MapLibre GL JS + GeoServer' },
        { module: '传输层 (TCP / UDP / TLS)', status: '完成', note: 'rustls 实现 HTTPS / WSS' }
      ]
    }
  },

  en: {
    meta: {
      title: 'Features · RustCam',
      description: 'The full RustCam capability list: live video, device ingestion, PTZ control, streaming clusters, electronic map, recording playback, alarms and access control.'
    },
    domains: [
      { id: 'video', icon: 'camera', title: 'Video services', summary: 'Live playback, multi-view layouts and recording playback', items: ['flv.js / hls.js / video.js playback', 'Single and multi-view layouts', 'Recording playback with timeline search', 'Visible live session state'] },
      { id: 'device', icon: 'broadcast', title: 'Device management', summary: 'Discovery, registration, status and grouping', items: ['GB28181 SIP registration', 'ONVIF WS-Discovery', 'Device groups and admin regions', 'Per-channel online status'] },
      { id: 'ptz', icon: 'route', title: 'PTZ control', summary: 'PTZ protocol support and preset management', items: ['GB28181 and ONVIF control', 'ContinuousMove / AbsoluteMove', 'Full preset CRUD', 'Returns 503 when offline and re-registers'] },
      { id: 'stream', icon: 'server', title: 'Media servers', summary: 'Clustered deployment with a unified adapter', items: ['Full support for mainstream media servers', 'Multi-vendor media server adapters', 'Weighted routing and enable/disable', 'Hook event handling'] },
      { id: 'map', icon: 'map-pin', title: 'Electronic map', summary: 'Map rendering, device markers and status filters', items: ['MapLibre GL JS rendering', 'GeoServer layers with WFS proxy', 'Marker clustering', 'Pick coordinates on the map'] },
      { id: 'record', icon: 'clock', title: 'Recording & playback', summary: 'Tasks, policies and file management', items: ['Start, stop, pause and resume tasks', 'Time-window recording policies', 'Recording files and play URLs', 'Parallel multi-channel playback'] },
      { id: 'alarm', icon: 'bell', title: 'Alarms', summary: 'Reporting, records and closed-loop handling', items: ['External alarm intake API', 'Query and paginate alarm records', 'Processing status flags', 'Alarm to session linkage'] },
      { id: 'auth', icon: 'lock', title: 'Users & permissions', summary: 'JWT authentication and RBAC', items: ['JWT login and token refresh', 'Casbin policy authorisation', 'Users / roles / permissions model', 'Dynamic role permission assignment'] }
    ],
    protocols: {
      title: 'Protocol coverage',
      columns: ['Protocol', 'Status', 'Capabilities'],
      rows: [
        { name: 'GB28181', status: 'Done', support: 'SIP signalling, stream ingest, PTZ, broadcast, two-way talk' },
        { name: 'ONVIF', status: 'Done', support: 'Discovery, PTZ control, Backchannel audio' },
        { name: 'RTSP', status: 'Done', support: 'TCP / UDP modes, RTP over RTSP' },
        { name: 'WebSocket', status: 'Done', support: 'Event push, two-way talk channel' }
      ]
    },
    progress: {
      title: 'Module progress',
      columns: ['Module', 'Status', 'Notes'],
      rows: [
        { module: 'GB28181 signalling / stream / PTZ / talk', status: 'Done', note: 'UDP and TCP modes' },
        { module: 'ONVIF discovery / PTZ / talk', status: 'Done', note: 'Broadcast probing with auto re-registration' },
        { module: 'RTSP stream ingest', status: 'Done', note: 'RTP over RTSP supported' },
        { module: 'Frontend live video / PTZ / video wall', status: 'Done', note: 'D-pad + joystick + dedicated preset card' },
        { module: 'Electronic map', status: 'Done', note: 'MapLibre GL JS + GeoServer' },
        { module: 'Transport TCP / UDP / TLS', status: 'Done', note: 'HTTPS / WSS via rustls' }
      ]
    }
  }
}
