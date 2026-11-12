export default {
  zh: {
    meta: {
      title: '产品特性 · RustCam',
      description: 'RustCam 的完整能力清单：实时视频、设备接入、云台控制、媒体集群、电子地图、录像回放、告警闭环与分级权限。'
    },
    domains: [
      { id: 'video', icon: 'camera', title: '视频服务', summary: '实时播放、多画面布局、录像回放', items: ['单画面与多画面布局切换', '多种播放格式自适应，浏览器直接观看', '实时会话状态可见', '录像回放与时间轴检索'] },
      { id: 'device', icon: 'broadcast', title: '设备管理', summary: '发现、注册、状态监控与分组', items: ['GB28181 设备注册上线即用', 'ONVIF 自动发现网络内设备', 'RTSP 直连接入第三方设备', '设备分组与行政区划', '通道级在线状态监控'] },
      { id: 'ptz', icon: 'route', title: '云台控制', summary: '转动、变倍与常用机位管理', items: ['方向与变倍控制', '常用机位一键调用', 'GB28181 与 ONVIF 双协议', '设备离线后自动恢复'] },
      { id: 'stream', icon: 'server', title: '媒体服务器', summary: '多机部署、自动调度与故障切换', items: ['主流媒体服务器完整支持', '多厂商适配，按权重分发', '故障自动切换', '启停与状态探测'] },
      { id: 'map', icon: 'map-pin', title: '电子地图', summary: '地图渲染、设备标记与状态筛选', items: ['所有点位摊在一张地图上', '点位聚合，密集区域不糊', '地图选点绑定通道坐标', '按在线状态筛选'] },
      { id: 'record', icon: 'clock', title: '录像与回放', summary: '录像任务、策略与文件管理', items: ['按区域与时段自动录像', '任务可暂停、恢复', '录像文件与播放地址', '多通道并行回放'] },
      { id: 'alarm', icon: 'bell', title: '告警', summary: '告警上报、记录与处理闭环', items: ['外部告警上报', '告警记录查询与分页', '处理状态标记', '告警与会话联动'] },
      { id: 'mobile', icon: 'navigation', title: '移动眼', summary: '无人机作为会动的视点接入同一套平台', items: ['主流飞控自动识别', '三维地球实时位置与姿态', '起飞、降落、返航、定点与云台指令', '航线规划与顺序执行', '禁飞区与越界告警', '机载摄像机关联后自动出流', '控制权租约、飞行日志与审计'] },
      { id: 'auth', icon: 'lock', title: '用户与权限', summary: '登录认证与分级授权', items: ['登录认证与令牌刷新', '用户 / 角色 / 权限三级模型', '角色继承，权限策略存数据库', '动态角色权限分配'] }
    ],
    protocols: {
      title: '协议兼容',
      columns: ['协议', '状态', '能做什么'],
      rows: [
        { name: 'GB28181', status: '完成', support: '主流摄像机与编码器接入、云台控制、平台广播与语音对讲' },
        { name: 'ONVIF', status: '完成', support: '网络内自动发现设备、云台转向变倍、语音对讲' },
        { name: 'RTSP', status: '完成', support: '任意可推流设备直连接入' },
        { name: 'WebSocket', status: '完成', support: '事件实时推送与语音对讲通道' },
        { name: 'MAVLink', status: '完成', support: '无人机飞控接入，位置姿态实时可见，指令有回执' }
      ]
    },
    progress: {
      title: '模块进度',
      columns: ['模块', '状态', '说明'],
      rows: [
        { module: 'GB28181 信令 / 流 / 云台 / 对讲', status: '完成', note: 'UDP 与 TCP 模式均支持' },
        { module: 'ONVIF 发现 / 云台 / 对讲', status: '完成', note: '自动探测与自动注册' },
        { module: 'RTSP 流接收', status: '完成', note: '支持 RTP over RTSP' },
        { module: '流媒体集群', status: '完成', note: 'ZLMediaKit 完整支持，其他厂商部分实现' },
        { module: '前端实时视频 / 云台 / 监控墙', status: '完成', note: '多画面布局、摇杆控制、布局拖拽' },
        { module: '电子地图', status: '完成', note: '点位聚合、选点绑定与状态筛选' },
        { module: '认证：登录令牌 + 角色权限', status: '完成', note: '用户 / 角色 / 权限三级模型' },
        { module: '角色继承与数据库驱动策略', status: '完成', note: '策略存数据库，支持角色继承与环检测' },
        { module: '传输层 TCP / UDP', status: '完成', note: '双传输模式' },
        { module: '传输层 TLS / HTTPS', status: '部分', note: '服务端未内置 TLS 监听，由前置反向代理终止' },
        { module: '移动眼三进程骨架', status: '完成', note: '控制面 / 机载适配 / 网关' },
        { module: '移动眼飞控接入与遥测', status: '完成', note: 'PX4 / ArduPilot 自动识别，指令回执闭环' },
        { module: '移动眼前端（档案 / 实时飞行 / 航线 / 围栏）', status: '完成', note: '2D / 3D 视图与轨迹回放' },
        { module: '移动眼机载视频接入', status: '完成', note: '关联摄像机后通道自动出流' },
        { module: '控制权仲裁 / 围栏风控 / 指令状态机', status: '完成', note: '租约跨实例互斥，围栏触发即产生告警' },
        { module: '移动眼与监控墙联动演示', status: '进行中', note: '模板化场景持续完善' }
      ]
    }
  },

  en: {
    meta: {
      title: 'Features · RustCam',
      description: 'The full RustCam capability list: live video, device onboarding, PTZ control, streaming clusters, an electronic map, recording playback, a closed alarm loop and role-based access.'
    },
    domains: [
      { id: 'video', icon: 'camera', title: 'Video services', summary: 'Live playback, multi-view layouts and recording playback', items: ['Single and multi-view layouts', 'Adaptive playback formats straight in the browser', 'Visible live session state', 'Recording playback with timeline search'] },
      { id: 'device', icon: 'broadcast', title: 'Device management', summary: 'Discovery, registration, status and grouping', items: ['GB28181 devices register and go live', 'ONVIF auto-discovers devices on the network', 'Direct RTSP onboarding for third-party devices', 'Device groups and administrative regions', 'Per-channel online status'] },
      { id: 'ptz', icon: 'route', title: 'PTZ control', summary: 'Movement, zoom and key positions', items: ['Pan, tilt and zoom', 'One-click recall of key positions', 'GB28181 and ONVIF', 'Automatic recovery after an outage'] },
      { id: 'stream', icon: 'server', title: 'Media servers', summary: 'Multiple servers, automatic scheduling and failover', items: ['Full support for mainstream media servers', 'Multi-vendor adapters with weighted routing', 'Automatic failover', 'Enable / disable and health probing'] },
      { id: 'map', icon: 'map-pin', title: 'Electronic map', summary: 'Map rendering, device markers and status filters', items: ['Every point on a single map', 'Clustered markers stay readable in dense areas', 'Pick coordinates directly on the map', 'Filter by online status'] },
      { id: 'record', icon: 'clock', title: 'Recording & playback', summary: 'Tasks, policies and file management', items: ['Automatic recording per zone and time window', 'Tasks can be paused and resumed', 'Recording files and play URLs', 'Parallel multi-channel playback'] },
      { id: 'alarm', icon: 'bell', title: 'Alarms', summary: 'Reporting, records and a closed loop', items: ['External alarm intake', 'Query and paginate alarm records', 'Processing status flags', 'Alarm to session linkage'] },
      { id: 'mobile', icon: 'navigation', title: 'Mobile Eye', summary: 'Drones join the platform as moving vantage points', items: ['Mainstream flight controllers detected automatically', 'Live position and attitude on a 3D globe', 'Takeoff, landing, return, hover and gimbal commands', 'Route planning and sequential execution', 'No-fly zones and breach alarms', 'Onboard cameras stream automatically once linked', 'Control leases, flight logs and audit'] },
      { id: 'auth', icon: 'lock', title: 'Users & permissions', summary: 'Sign-in and tiered authorisation', items: ['Sign-in with token refresh', 'Users / roles / permissions model', 'Role inheritance with policies stored in the database', 'Dynamic role permission assignment'] }
    ],
    protocols: {
      title: 'Protocol coverage',
      columns: ['Protocol', 'Status', 'What it does'],
      rows: [
        { name: 'GB28181', status: 'Done', support: 'Mainstream cameras and encoders, PTZ control, platform broadcast and two-way talk' },
        { name: 'ONVIF', status: 'Done', support: 'Automatic discovery on the network, PTZ movement and zoom, two-way talk' },
        { name: 'RTSP', status: 'Done', support: 'Direct onboarding for anything that can push a stream' },
        { name: 'WebSocket', status: 'Done', support: 'Real-time event push and a two-way talk channel' },
        { name: 'MAVLink', status: 'Done', support: 'Drone flight control with live position, attitude and acknowledged commands' }
      ]
    },
    progress: {
      title: 'Module progress',
      columns: ['Module', 'Status', 'Notes'],
      rows: [
        { module: 'GB28181 signalling / stream / PTZ / talk', status: 'Done', note: 'Both UDP and TCP modes' },
        { module: 'ONVIF discovery / PTZ / talk', status: 'Done', note: 'Automatic probing and registration' },
        { module: 'RTSP stream ingest', status: 'Done', note: 'RTP over RTSP supported' },
        { module: 'Streaming cluster', status: 'Done', note: 'ZLMediaKit fully supported, other vendors partially' },
        { module: 'Frontend live video / PTZ / video wall', status: 'Done', note: 'Multi-view layouts, joystick control, drag ordering' },
        { module: 'Electronic map', status: 'Done', note: 'Clustered markers, coordinate picking and status filters' },
        { module: 'Authentication: tokens and role permissions', status: 'Done', note: 'Users / roles / permissions model' },
        { module: 'Role inheritance with database-driven policies', status: 'Done', note: 'Policies stored in the database, with inheritance and cycle detection' },
        { module: 'Transport TCP / UDP', status: 'Done', note: 'Both transport modes' },
        { module: 'Transport TLS / HTTPS', status: 'Partial', note: 'No built-in TLS listener on the server — terminate it with a reverse proxy' },
        { module: 'Mobile Eye three-process skeleton', status: 'Done', note: 'Control plane / airborne adapter / gateway' },
        { module: 'Mobile Eye flight-control integration and telemetry', status: 'Done', note: 'PX4 / ArduPilot detected automatically, commands acknowledged' },
        { module: 'Mobile Eye console (profiles / live flight / routes / geofences)', status: 'Done', note: '2D and 3D views with track replay' },
        { module: 'Mobile Eye onboard video', status: 'Done', note: 'Channels stream automatically once a camera is linked' },
        { module: 'Control authority / geofencing / command state machine', status: 'Done', note: 'Leases are exclusive across instances; a fence breach raises an alarm' },
        { module: 'Mobile Eye and video wall integration demo', status: 'In progress', note: 'Scenario templates keep being polished' }
      ]
    }
  }
}
