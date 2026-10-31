export default {
  zh: {
    meta: {
      title: '应用场景 · RustCam',
      description: 'RustCam 在园区安防、城市治理、工业安防与交通枢纽中的落地方式：痛点、能力映射与收益指标。'
    },
    items: [
      {
        id: 'campus',
        icon: 'grid',
        title: '园区安防',
        pain: '不同品牌摄像机分散在弱电井与楼层，告警靠人工巡检，事件定位耗时以分钟计。',
        mapping: ['GB28181 / ONVIF / RTSP 统一接入', '电子地图 + 设备点位聚合', '监控墙布局与拖拽编排', '录像策略按区域与时段执行'],
        outcome: '事件定位从分钟级降到秒级；新增设备只需配置地址，无需二次开发。'
      },
      {
        id: 'city',
        icon: 'globe',
        title: '城市治理',
        pain: '区域级视频资源分散在多个区县平台，跨域调阅需要人工申请，权限边界模糊。',
        mapping: ['GB28181 级联接入与平台广播', '用户 / 角色 / 权限三级模型', 'JWT + Casbin 细粒度授权', '行政区划与设备分组'],
        outcome: '跨域调阅流程线上化，权限由策略集中管控，审计有据可查。'
      },
      {
        id: 'industry',
        icon: 'cpu',
        title: '工业安防',
        pain: '生产现场与办公区域权限混用，报警事件缺少处理闭环，责任难追溯。',
        mapping: ['告警上报与处理状态标记', '通道级状态监控', '异常录像策略与回放取证', '客户端本地实时预览'],
        outcome: '告警从「发生」到「处理」全程留痕，事后取证链路完整。'
      },
      {
        id: 'transport',
        icon: 'route',
        title: '交通枢纽',
        pain: '高峰时段并发通道数激增，实时流与录像任务争抢带宽，服务质量难以保障。',
        mapping: ['多服务器权重调度与故障切换', '媒体服务器集群化部署', 'TLS 加密传输（HTTPS / WSS）', 'TCP / UDP 双传输模式'],
        outcome: '并发能力随流媒体集群线性扩展，单点故障不再中断业务。'
      }
    ]
  },

  en: {
    meta: {
      title: 'Scenarios · RustCam',
      description: 'How RustCam is applied to campus security, city operations, industrial safety and transport hubs: pain points, capability mapping and measurable outcomes.'
    },
    items: [
      {
        id: 'campus',
        icon: 'grid',
        title: 'Campus security',
        pain: 'Cameras from different vendors are scattered across risers and floors; alarms rely on manual patrol and incidents take minutes to locate.',
        mapping: ['Unified GB28181 / ONVIF / RTSP ingestion', 'Electronic map with clustered markers', 'Video wall layout with drag ordering', 'Recording policies per zone and time window'],
        outcome: 'Incident location drops from minutes to seconds; new devices only need configuration, no extra development.'
      },
      {
        id: 'city',
        icon: 'globe',
        title: 'City operations',
        pain: 'Regional video resources are split across district platforms; cross-domain access needs manual approval and permission boundaries are blurry.',
        mapping: ['GB28181 cascading and platform broadcast', 'Users / roles / permissions model', 'JWT + Casbin fine-grained authorisation', 'Administrative regions and device groups'],
        outcome: 'Cross-domain access goes online, permissions are centrally governed, and every action is auditable.'
      },
      {
        id: 'industry',
        icon: 'cpu',
        title: 'Industrial safety',
        pain: 'Production floors and offices share permissions, alarms have no closed loop, and responsibility cannot be traced.',
        mapping: ['Alarm intake and processing status', 'Per-channel status monitoring', 'Exception recording policies and playback', 'Local live preview on the desktop client'],
        outcome: 'Every alarm is traceable from occurrence to resolution, with a complete chain for later evidence.'
      },
      {
        id: 'transport',
        icon: 'route',
        title: 'Transport hubs',
        pain: 'Concurrent channels spike during rush hours; live streams and recording tasks compete for bandwidth.',
        mapping: ['Weighted multi-server routing with failover', 'Clustered Media Server deployment', 'TLS protected transport (HTTPS / WSS)', 'Dual TCP / UDP transport modes'],
        outcome: 'Capacity scales linearly with the streaming cluster and a single point of failure no longer interrupts service.'
      }
    ]
  }
}
