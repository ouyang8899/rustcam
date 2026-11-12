export default {
  zh: {
    meta: {
      title: 'RustCam · 开源的私有化视频监控平台',
      description: 'RustCam 把分散在不同品牌、不同楼层的摄像机、编码器与无人机接入同一个平台：实时观看、录像回放、云台控制、电子地图、告警闭环一站具备，支持私有化部署与开源二次开发。'
    },
    hero: {
      eyebrow: '开源 · 私有化部署 · MIT 协议',
      title: '固定机位 + 移动视点\n一套平台管到底',
      subtitle: '把分散在不同品牌、不同楼层的摄像机、编码器和报警设备收进一个平台：画面随时看，录像自动留，事件一键定位到点位；固定机位之外，无人机等会动的视点也能按需飞过去看。',
      primaryCta: '查看文档',
      secondaryCta: '获取源码',
      badge: {
        title: '数据留在自己的服务器',
        note: '开源 MIT 协议，可自行二次开发'
      },
      stats: [
        { value: 5, suffix: ' 大', label: '协议族接入' },
        { value: 9, suffix: ' 大', label: '能力域' },
        { value: 3, suffix: ' 端', label: 'Web / 桌面 / 服务端' },
        { value: 100, suffix: '%', label: '私有化部署' }
      ]
    },
    protocols: {
      title: '设备接入',
      description: '主流安防设备、编码器与无人机飞控都能接进来，仓库里的设备不用换。',
      items: [
        { name: 'GB28181', desc: '国内主流摄像机与编码器的通用接入方式，注册上线即可纳管。' },
        { name: 'ONVIF', desc: '自动发现网络里的摄像机，支持云台转向、变倍与语音对讲。' },
        { name: 'RTSP', desc: '只要能推流就能接入，覆盖大量第三方品牌与自研编码器。' },
        { name: 'WebSocket', desc: '报警与设备状态实时推送到控制台，不靠人工轮询。' },
        { name: 'MAVLink', desc: '无人机飞控接入，位置、姿态与机载画面统一进平台。' }
      ]
    },
    features: {
      eyebrow: '核心能力',
      title: '从接设备到取证，一个平台走完',
      description: '不用在多个系统之间来回切换，也不用为不同品牌各搭一套。',
      items: [
        { icon: 'camera', title: '实时视频', description: '多画面同时看，需要时放大到关键点位。', items: ['单画面与多画面布局切换', '多种播放格式自适应', '谁在看、看了多久一目了然'] },
        { icon: 'broadcast', title: '设备接入', description: '不同品牌、不同协议的设备统一纳管，新设备只要填个地址。', items: ['设备自动发现与注册', '按区域与用途分组', '通道在线状态实时可见'] },
        { icon: 'route', title: '云台控制', description: '在地图或画面上直接转动摄像机、变倍、回到预置位。', items: ['方向与变倍控制', '常用机位一键调用', '设备离线后自动恢复'] },
        { icon: 'map-pin', title: '电子地图', description: '所有点位摊在一张地图上，点开就看画面，事件位置一眼找到。', items: ['点位聚合，密而不乱', '地图选点绑定设备位置', '按在线状态筛选'] },
        { icon: 'clock', title: '录像与回放', description: '录像按策略自动留存，需要时按时间和通道调出来。', items: ['按区域与时段自动录像', '多通道并行回放', '关键片段快速定位'] },
        { icon: 'server', title: '媒体集群', description: '点位多了也不卡：按负载自动分发，单点故障不影响整体。', items: ['多台媒体服务器自动调度', '故障自动切换', '运行状态与启停可控'] }
      ]
    },
    mobileEye: {
      eyebrow: '移动眼 · MOBILE EYE',
      title: '固定摄像机看住该看的，移动眼盯住会变的。',
      description: '无人机不是「会飞的摄像头」：它自带飞控，决定去哪看、怎么看、何时回。RustCam 把整套飞行能力接进监控域——三维地球上的实时位置、航线规划、电子围栏与飞行指令，与固定摄像机共用同一套录像、证据链与监控墙。',
      principles: [
        { title: '不新增固定点位', desc: '需要时才调度：省掉布线、杆件、点位与后续维护，视觉覆盖按需扩展，而不是提前埋满。' },
        { title: '不锁场景', desc: '航线、围栏、告警规则以配置组合而成；巡检 / 安防补盲 / 应急取证是同一套能力的不同模板。' }
      ],
      capabilities: ['主流飞控开箱即用', '三维地球实时位置与姿态', '航线规划与自动执行', '电子围栏与越界告警', '机载摄像机自动出流', '多人操作互不打架', '飞行日志全程留痕', '内置模拟器，无需真机体验'],
      cta: '查看文档',
      status: '已落地：飞控接入、实时飞行视图、无人机档案、机载摄像机自动出流、航线、围栏、告警审计与一键测试环境。',
      hud: {
        title: 'DRONE-01 · 移动眼',
        link: '飞控已连接',
        alt: '无人机飞控视锥锁定移动目标的示意图',
        caption: '飞控决定去哪看：位置、姿态、航线与围栏实时可视',
        readouts: [
          { key: 'heading', label: 'HDG' },
          { key: 'altitude', label: 'ALT' },
          { key: 'battery', label: 'BAT' },
          { key: 'distance', label: 'DIST' }
        ]
      }
    },
    architecture: {
      eyebrow: '系统架构',
      title: '一套后台，管住所有设备',
      description: '不管是固定摄像机还是无人机，共用同一套后台、同一份录像、同一套权限与告警——机种不同，操作方式不变。',
      points: [
        { title: '设备统一纳管', desc: '不同协议、不同厂商的设备在后台是同一张清单，新增设备不需要改编业务流程。' },
        { title: '录像与告警不分机种', desc: '固定机位与无人机产生的画面和事件共用同一条证据链，取证时不用拼时间线。' },
        { title: '规模按需扩展', desc: '从单台设备到上千路并发，媒体服务可以集群部署，业务与运维方式保持不变。' }
      ]
    },
    gallery: {
      eyebrow: '界面预览',
      title: 'Web 控制台与桌面客户端',
      description: '均为项目真实运行界面，点击可放大。',
      web: {
        title: 'Web 控制台',
        items: [
          { src: '/img/console-overview.webp', alt: '系统概览面板', caption: '系统概览', width: 1920, height: 979 },
          { src: '/img/console-devices.webp', alt: '设备列表', caption: '设备列表', width: 1920, height: 1082 },
          { src: '/img/console-map-video.webp', alt: '电子地图上直接查看点位画面', caption: '电子地图 · 点开即看', width: 1920, height: 1082 },
          { src: '/img/console-tvwall.webp', alt: '监控墙', caption: '监控墙', width: 1920, height: 1082 },
          { src: '/img/console-playback.webp', alt: '录像回放', caption: '录像回放', width: 1920, height: 1082 },
          { src: '/img/console-alarms.webp', alt: '报警记录', caption: '报警记录 · 处理闭环', width: 1920, height: 1082 },
          { src: '/img/console-media-servers.webp', alt: '媒体服务器管理', caption: '媒体服务器 · 集群调度', width: 1920, height: 1082 },
          { src: '/img/console-users.webp', alt: '用户与权限管理', caption: '用户与权限', width: 1920, height: 979 },
          { src: '/img/console-channels.webp', alt: '通道列表', caption: '通道列表', width: 1920, height: 1082 }
        ]
      },
      mobile: {
        title: '移动眼控制台',
        items: [
          { src: '/img/mobile-eye-live.webp', alt: '移动眼实时飞行', caption: '实时飞行', width: 1600, height: 812 },
          { src: '/img/mobile-eye-drones.webp', alt: '移动眼无人机档案', caption: '无人机档案', width: 1600, height: 816 },
          { src: '/img/mobile-eye-route.webp', alt: '移动眼航线规划', caption: '航线规划', width: 1600, height: 816 },
          { src: '/img/mobile-eye-fence.webp', alt: '移动眼电子围栏', caption: '电子围栏', width: 1600, height: 812 },
          { src: '/img/mobile-eye-track.webp', alt: '移动眼轨迹回放', caption: '轨迹回放', width: 1600, height: 816 },
          { src: '/img/mobile-eye-control.webp', alt: '移动眼控制权管理', caption: '控制权管理', width: 1600, height: 816 }
        ]
      },
      desktop: {
        title: 'Qt 桌面客户端',
        items: [
          { src: '/img/desktop-live.webp', alt: '客户端实时预览', caption: '实时预览', width: 1920, height: 1041 },
          { src: '/img/desktop-map.webp', alt: '客户端电子地图', caption: '电子地图', width: 1920, height: 1041 },
          { src: '/img/desktop-playback.webp', alt: '客户端录像回放', caption: '录像回放', width: 1920, height: 1041 }
        ]
      }
    },
    scenarios: {
      eyebrow: '应用场景',
      title: '从园区到城市',
      description: '把分散的摄像机、编码器与报警设备收敛到一个可运营的平台。',
      items: [
        { icon: 'grid', title: '园区安防', desc: '不同品牌的摄像机统一纳管，地图加监控墙快速定位事件。' },
        { icon: 'globe', title: '城市治理', desc: '把区域视频资源汇聚到统一平台，跨域调阅线上化。' },
        { icon: 'cpu', title: '工业安防', desc: '生产现场与办公区域权限隔离，报警记录闭环处理。' },
        { icon: 'route', title: '交通枢纽', desc: '高峰时段并发接入，录像按时间段自动执行。' }
      ]
    },
    quickStart: {
      eyebrow: '快速开始',
      title: '三步接管你的监控',
      description: '在自己的服务器上跑起来，数据留在自己手里。',
      steps: [
        { title: '拉取源码', code: 'git clone https://gitee.com/ouliuying/rustcam.git\ncd rustcam', lang: 'bash' },
        { title: '准备数据库与缓存', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: '构建并启动', code: 'cargo build --release -p rustcam-server\n./target/release/rustcam-media', lang: 'bash' }
      ],
      frontendTitle: '启动 Web 控制台',
      frontendCode: 'cd frontend\nnpm install\nnpm run dev',
      note: '也可以使用 Qt 桌面客户端面向运维岗；完整说明见开发者文档。'
    },
    techStack: {
      eyebrow: '为什么选它',
      title: '私有化、可掌控、可扩展',
      description: '不按路数按月收费，也不把视频交给第三方。',
      columns: [
        { title: '私有化部署', items: ['数据全部留在自有服务器', '内网即可运行，不依赖公网', '支持反向代理与 HTTPS 接入'] },
        { title: '开箱即用', items: ['一套控制台覆盖 Web 与桌面两端', '主流设备协议开箱支持', '内置测试环境，无需真实设备即可演练'] },
        { title: '可长期维护', items: ['MIT 协议开源，可自行二次开发', '模块化设计，可按需扩充设备类型', '单机起步，集群扩展不改用法'] }
      ]
    },
    cta: {
      title: '让监控回到一个平台里',
      description: '开源、MIT 协议、文档齐全。可以先看文档评估，也可以直接克隆源码，在自己的环境里跑起来。',
      primary: '查看开发者文档',
      secondary: '获取源码'
    }
  },

  en: {
    meta: {
      title: 'RustCam · An open-source, self-hosted video surveillance platform',
      description: 'RustCam brings cameras, encoders and drones from different vendors and floors into one platform: live viewing, recording playback, PTZ control, an electronic map and a closed alarm loop — deploy it on your own infrastructure and extend it yourself.'
    },
    hero: {
      eyebrow: 'OPEN SOURCE · SELF-HOSTED · MIT LICENCE',
      title: 'Fixed cameras and moving eyes,\none platform to run them all',
      subtitle: 'Pull the cameras, encoders and alarm devices scattered across vendors and floors into a single platform: watch any feed, keep recordings on schedule, jump straight to the incident location — and when a fixed point is not enough, send a drone to look.',
      primaryCta: 'Read the docs',
      secondaryCta: 'Get the source',
      badge: {
        title: 'Your data stays on your server',
        note: 'Open source under MIT — extend it yourself'
      },
      stats: [
        { value: 5, suffix: '', label: 'protocol families' },
        { value: 9, suffix: '', label: 'capability domains' },
        { value: 3, suffix: '', label: 'web / desktop / server' },
        { value: 100, suffix: '%', label: 'self-hosted deployment' }
      ]
    },
    protocols: {
      title: 'Device onboarding',
      description: 'Mainstream cameras, encoders and drone flight controllers all plug in — the hardware you already own keeps working.',
      items: [
        { name: 'GB28181', desc: 'The common way mainland cameras and encoders join a platform: register once, and they are managed.' },
        { name: 'ONVIF', desc: 'Discovers cameras on the network automatically, with PTZ movement, zoom and two-way talk.' },
        { name: 'RTSP', desc: 'Anything that can push a stream connects directly, covering most third-party vendors.' },
        { name: 'WebSocket', desc: 'Alarms and device status reach the console in real time, with no manual polling.' },
        { name: 'MAVLink', desc: 'Drone flight control joins the platform, bringing position, attitude and onboard video with it.' }
      ]
    },
    features: {
      eyebrow: 'Core capabilities',
      title: 'From onboarding to evidence, in one platform',
      description: 'No more hopping between systems, and no separate build for every vendor.',
      items: [
        { icon: 'camera', title: 'Live video', description: 'Watch several feeds at once and zoom into the one that matters.', items: ['Single and multi-view layouts', 'Adaptive playback formats', 'See who is watching and for how long'] },
        { icon: 'broadcast', title: 'Device onboarding', description: 'Cameras from every vendor and protocol land in one list — a new device only needs an address.', items: ['Automatic discovery and registration', 'Grouping by area and purpose', 'Per-channel online status'] },
        { icon: 'route', title: 'PTZ control', description: 'Turn, zoom and recall presets from the map or from the picture itself.', items: ['Pan, tilt and zoom', 'One-click recall of key positions', 'Automatic recovery after an outage'] },
        { icon: 'map-pin', title: 'Electronic map', description: 'Every point on one map: open it to see the picture, and find an incident location at a glance.', items: ['Clustered markers stay readable', 'Pick coordinates directly on the map', 'Filter by online status'] },
        { icon: 'clock', title: 'Recording & playback', description: 'Recordings are kept on schedule and pulled back by time and channel when needed.', items: ['Automatic recording per zone and window', 'Parallel multi-channel playback', 'Fast jump to key moments'] },
        { icon: 'server', title: 'Streaming cluster', description: 'More cameras do not mean stutter: work is spread by load and a single failure changes nothing.', items: ['Automatic scheduling across media servers', 'Failover on failure', 'Status visibility and manual control'] }
      ]
    },
    mobileEye: {
      eyebrow: 'MOBILE EYE',
      title: 'Fixed cameras watch what matters; the mobile eye watches what moves.',
      description: 'A drone is not "a camera that flies": it carries its own flight control and decides where to look, how to look and when to return. RustCam pulls that flight capability into the surveillance domain — live position on a 3D globe, route planning, geofences and flight commands share the same recording, evidence chain and video wall as fixed cameras.',
      principles: [
        { title: 'No new fixed points', desc: 'Dispatch only when it is needed: no cabling, no poles, no permanent points, and coverage that grows on demand instead of being buried in advance.' },
        { title: 'No scenario lock-in', desc: 'Routes, geofences and alarm rules are composed by configuration; patrol, blind-spot coverage and emergency response are templates of one capability set.' }
      ],
      capabilities: ['Mainstream flight controllers out of the box', 'Live position and attitude on a 3D globe', 'Route planning and automatic execution', 'Geofences with breach alarms', 'Automatic onboard camera streams', 'No conflicts between operators', 'Full flight log trail', 'Built-in simulator — no hardware needed'],
      cta: 'Read the docs',
      status: 'Delivered: flight control, live flight view, drone profiles, automatic onboard camera streams, routes, geofences, alert audit and a one-command test environment.',
      hud: {
        title: 'DRONE-01 · MOBILE EYE',
        link: 'Flight control connected',
        alt: 'Illustration of a drone vision cone locking onto a moving target',
        caption: 'Flight control decides where to look: position, attitude, routes and geofences on a 3D globe',
        readouts: [
          { key: 'heading', label: 'HDG' },
          { key: 'altitude', label: 'ALT' },
          { key: 'battery', label: 'BAT' },
          { key: 'distance', label: 'DIST' }
        ]
      }
    },
    architecture: {
      eyebrow: 'System architecture',
      title: 'One backend for every device',
      description: 'Fixed cameras and drones share the same backend, the same recordings and the same permissions and alarms — the device type changes, the way you work does not.',
      points: [
        { title: 'One device list', desc: 'Cameras from any vendor or protocol appear as a single inventory, and onboarding a new device never means rewriting business logic.' },
        { title: 'One evidence chain', desc: 'Footage and events from fixed cameras and drones land on the same timeline, so investigations never need to stitch sources together.' },
        { title: 'Scale on demand', desc: 'From one device to thousands of concurrent streams, the media tier runs as a cluster while operations stay the same.' }
      ]
    },
    gallery: {
      eyebrow: 'Interface preview',
      title: 'Web console and desktop client',
      description: 'Every screenshot below comes from a real running instance — click to zoom.',
      web: {
        title: 'Web console',
        items: [
          { src: '/img/console-overview.webp', alt: 'System overview', caption: 'System overview', width: 1920, height: 979 },
          { src: '/img/console-devices.webp', alt: 'Device list', caption: 'Device list', width: 1920, height: 1082 },
          { src: '/img/console-map-video.webp', alt: 'Watching a camera directly on the electronic map', caption: 'Electronic map · open and watch', width: 1920, height: 1082 },
          { src: '/img/console-tvwall.webp', alt: 'Video wall', caption: 'Video wall', width: 1920, height: 1082 },
          { src: '/img/console-playback.webp', alt: 'Recording playback', caption: 'Recording playback', width: 1920, height: 1082 },
          { src: '/img/console-alarms.webp', alt: 'Alarm records', caption: 'Alarms · closed loop', width: 1920, height: 1082 },
          { src: '/img/console-media-servers.webp', alt: 'Media server management', caption: 'Media servers · cluster scheduling', width: 1920, height: 1082 },
          { src: '/img/console-users.webp', alt: 'User and permission management', caption: 'Users & permissions', width: 1920, height: 979 },
          { src: '/img/console-channels.webp', alt: 'Channel list', caption: 'Channel list', width: 1920, height: 1082 }
        ]
      },
      mobile: {
        title: 'Mobile Eye console',
        items: [
          { src: '/img/mobile-eye-live.webp', alt: 'Mobile Eye live flight', caption: 'Live flight', width: 1600, height: 812 },
          { src: '/img/mobile-eye-drones.webp', alt: 'Mobile Eye drone profiles', caption: 'Drone profiles', width: 1600, height: 816 },
          { src: '/img/mobile-eye-route.webp', alt: 'Mobile Eye route planning', caption: 'Route planning', width: 1600, height: 816 },
          { src: '/img/mobile-eye-fence.webp', alt: 'Mobile Eye geofence editor', caption: 'Geofences', width: 1600, height: 812 },
          { src: '/img/mobile-eye-track.webp', alt: 'Mobile Eye track replay', caption: 'Track replay', width: 1600, height: 816 },
          { src: '/img/mobile-eye-control.webp', alt: 'Mobile Eye control authority', caption: 'Control authority', width: 1600, height: 816 }
        ]
      },
      desktop: {
        title: 'Qt desktop client',
        items: [
          { src: '/img/desktop-live.webp', alt: 'Desktop live preview', caption: 'Live preview', width: 1920, height: 1041 },
          { src: '/img/desktop-map.webp', alt: 'Desktop electronic map', caption: 'Electronic map', width: 1920, height: 1041 },
          { src: '/img/desktop-playback.webp', alt: 'Desktop recording playback', caption: 'Recording playback', width: 1920, height: 1041 }
        ]
      }
    },
    scenarios: {
      eyebrow: 'Scenarios',
      title: 'From a campus to a city',
      description: 'Pull scattered cameras, encoders and alarm devices into one operable platform.',
      items: [
        { icon: 'grid', title: 'Campus security', desc: 'Onboard cameras from any vendor, and locate incidents fast with the map and the video wall.' },
        { icon: 'globe', title: 'City operations', desc: 'Aggregate regional video resources into one platform and take cross-domain access online.' },
        { icon: 'cpu', title: 'Industrial safety', desc: 'Separate permissions between production floors and offices, and close the loop on every alarm.' },
        { icon: 'route', title: 'Transport hubs', desc: 'High-concurrency live channels, with recording policies driven by time schedules.' }
      ]
    },
    quickStart: {
      eyebrow: 'Quick start',
      title: 'Take over your surveillance in three steps',
      description: 'Run it on your own server and keep the data where it belongs.',
      steps: [
        { title: 'Clone the source', code: 'git clone https://gitee.com/ouliuying/rustcam.git\ncd rustcam', lang: 'bash' },
        { title: 'Prepare database and cache', code: 'psql -U postgres -c "CREATE DATABASE rustcam;"\npsql -U postgres -d rustcam -f sql/schema.sql', lang: 'bash' },
        { title: 'Build and run', code: 'cargo build --release -p rustcam-server\n./target/release/rustcam-media', lang: 'bash' }
      ],
      frontendTitle: 'Start the web console',
      frontendCode: 'cd frontend\nnpm install\nnpm run dev',
      note: 'A Qt desktop client is available for on-site operators — see the developer docs for the full setup.'
    },
    techStack: {
      eyebrow: 'Why choose it',
      title: 'Self-hosted, in your control, ready to grow',
      description: 'No per-channel subscription, and your footage never leaves your infrastructure.',
      columns: [
        { title: 'Self-hosted', items: ['All data stays on your own server', 'Runs inside the intranet, no public network needed', 'Reverse proxy and HTTPS supported'] },
        { title: 'Ready out of the box', items: ['One console for web and desktop', 'Mainstream device protocols supported', 'Built-in test environment — no real hardware needed'] },
        { title: 'Built to last', items: ['MIT licensed and open source', 'Modular design, extend the device types you need', 'Start on one box, scale to a cluster without changing habits'] }
      ]
    },
    cta: {
      title: 'Bring your surveillance back into one platform',
      description: 'Open source, MIT licensed and fully documented. Read the docs to evaluate it, or clone the source and run it in your own environment.',
      primary: 'Read the developer docs',
      secondary: 'Get the source'
    }
  }
}
