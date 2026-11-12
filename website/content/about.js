export default {
  zh: {
    meta: {
      title: '关于 · RustCam',
      description: 'RustCam 的项目定位、产品原则、路线图、贡献指南与开源许可。'
    },
    intro: {
      title: '关于 RustCam',
      desc: 'RustCam-Media 是一套可私有化部署的视频监控平台：把不同品牌、不同楼层的摄像机、编码器与无人机接入同一个后台，让实时观看、录像留存、告警处置与事后取证在同一个界面里完成。开源 MIT 协议，可自行部署与二次开发。'
    },
    principles: {
      title: '产品原则',
      items: [
        { title: '私有化优先', desc: '视频不出内网，数据留在客户自己的服务器上；不按路数按月收费，也不把画面交给第三方。' },
        { title: '一套后台，一种用法', desc: '固定摄像机与无人机共用同一份录像、同一条告警流和同一套权限，机种不同，操作方式不变。' },
        { title: '兼容存量设备', desc: '主流协议开箱支持，仓库里的设备不用换；新增设备只需填个地址，不需要二次开发。' },
        { title: '移动眼按需调度', desc: '需要时才飞过去看，省掉布线、杆件与长期维护；航线、围栏与告警规则由配置组合，不锁死在某个场景。' },
        { title: '安全默认开启', desc: '登录令牌与分级权限贯穿全部业务接口，权限边界与操作审计开箱可用。' }
      ]
    },
    roadmap: {
      title: '路线图',
      items: [
        { status: 'done', title: 'GB28181 全链路', desc: '主流摄像机与编码器接入、云台控制、平台广播与语音对讲均已完成。' },
        { status: 'done', title: 'ONVIF 发现与控制', desc: '自动发现网络内设备、转动变倍与语音对讲已完成。' },
        { status: 'done', title: 'RTSP 与媒体服务', desc: '第三方设备接入与媒体服务集群已完成，故障自动切换。' },
        { status: 'progress', title: '媒体服务多厂商适配', desc: '适配层已抽象完毕，部分厂商的实现仍在推进中。' },
        { status: 'done', title: '移动眼', desc: '无人机作为会动的视点接入监控域：飞控接入、三维实时视图、机载摄像机自动出流、航线、围栏、控制权与审计已落地，并内置模拟器测试环境。' },
        { status: 'next', title: '能力持续演进', desc: '模板化场景、移动眼与监控墙联动演示，以及录像策略、地图选点等能力的持续迭代。' }
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
      description: 'RustCam project positioning, product principles, roadmap, contribution guide and open-source licence.'
    },
    intro: {
      title: 'About RustCam',
      desc: 'RustCam-Media is a self-hostable video surveillance platform: cameras, encoders and drones from different vendors and floors all join one backend, so live viewing, retention, alarm handling and later evidence work happen in a single console. Released under the MIT licence, you can deploy and extend it yourself.'
    },
    principles: {
      title: 'Product principles',
      items: [
        { title: 'Self-hosting first', desc: 'Video never leaves the intranet and data stays on your own server — no per-channel subscription, no third party holding your footage.' },
        { title: 'One backend, one way of working', desc: 'Fixed cameras and drones share the same recordings, the same alarm stream and the same permissions; the device type changes, the workflow does not.' },
        { title: 'Works with what you own', desc: 'Mainstream protocols are supported out of the box, so the hardware you already have keeps working and a new device only needs an address.' },
        { title: 'Dispatch on demand', desc: 'Send a drone only when it is needed, saving cabling, poles and long-term maintenance; routes, geofences and alarm rules are composed by configuration instead of locking you into one scenario.' },
        { title: 'Secure by default', desc: 'Tokens and tiered permissions guard every business endpoint, with permission boundaries and action auditing available out of the box.' }
      ]
    },
    roadmap: {
      title: 'Roadmap',
      items: [
        { status: 'done', title: 'Full GB28181 chain', desc: 'Mainstream camera and encoder onboarding, PTZ control, platform broadcast and two-way talk are all complete.' },
        { status: 'done', title: 'ONVIF discovery and control', desc: 'Automatic discovery on the network, movement, zoom and two-way talk are complete.' },
        { status: 'done', title: 'RTSP and media services', desc: 'Third-party device onboarding and the media server cluster are complete, with automatic failover.' },
        { status: 'progress', title: 'Multi-vendor media adapters', desc: 'The adapter layer is finished; some vendor integrations are still partially implemented.' },
        { status: 'done', title: 'Mobile Eye', desc: 'Drones join the surveillance domain as moving vantage points: flight control, a live 3D view, automatic onboard camera streams, routes, geofences, control authority and audit are delivered, with a built-in simulator test environment.' },
        { status: 'next', title: 'Continuous evolution', desc: 'Scenario templates, the Mobile Eye and video wall integration demo, plus recording policies and map picking keep iterating.' }
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
