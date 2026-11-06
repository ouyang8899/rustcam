import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from '../stores/authStore'

const routes = [
  {
    path: '/',
    component: () => import('../components/layout/AppLayout.vue'),
    meta: { requiresAuth: true },
    children: [
      {
        path: '',
        name: 'Dashboard',
        component: () => import('../views/Dashboard.vue'),
      },
      {
        path: 'devices',
        name: 'Devices',
        component: () => import('../views/Devices.vue'),
      },
      {
        path: 'device-map',
        name: 'DeviceMap',
        component: () => import('../views/DeviceMap.vue'),
      },
      {
        path: 'devices/:deviceTag',
        name: 'DeviceDetail',
        component: () => import('../views/DeviceDetail.vue'),
      },
      {
        path: 'channels',
        name: 'Channels',
        component: () => import('../views/Channels.vue'),
      },
      {
        path: 'channels/:deviceTag/:channelTag',
        name: 'ChannelDetail',
        component: () => import('../views/ChannelDetail.vue'),
      },
      {
        path: 'streams',
        name: 'Streams',
        component: () => import('../views/Streams.vue'),
      },
      {
        path: 'streams/:id',
        name: 'StreamDetail',
        component: () => import('../views/StreamDetail.vue'),
      },
      {
        path: 'sessions',
        name: 'Sessions',
        component: () => import('../views/Sessions.vue'),
      },
      {
        path: 'public',
        name: 'Public',
        component: () => import('../views/PublicCameras.vue'),
      },
      {
        path: 'settings',
        name: 'Settings',
        component: () => import('../views/Settings.vue'),
      },
      {
        path: 'servers',
        name: 'Servers',
        component: () => import('../views/MediaServers.vue'),
      },
      {
        path: 'recordings',
        name: 'Recordings',
        component: () => import('../views/Recordings.vue'),
      },
      {
        path: 'recordings/files',
        name: 'RecordingFiles',
        component: () => import('../views/RecordingFiles.vue'),
      },
      {
        path: 'recording-policies',
        name: 'RecordingPolicies',
        component: () => import('../views/RecordingPolicies.vue'),
      },
      {
        path: 'recording-playback',
        name: 'RecordingPlayback',
        component: () => import('../views/RecordingPlayback.vue'),
      },
      {
        path: 'users',
        name: 'UserManagement',
        component: () => import('../views/UserManagement.vue'),
      },
      {
        path: 'roles',
        name: 'RoleManagement',
        component: () => import('../views/RoleManagement.vue'),
      },
      {
        path: 'permissions',
        name: 'PermissionManagement',
        component: () => import('../views/PermissionManagement.vue'),
      },
      {
        path: 'alarms',
        name: 'Alarms',
        component: () => import('../views/Alarms.vue'),
      },
      {
        path: 'drone',
        redirect: '/drone/drones',
      },
      {
        // 规则 §10.2 的 /drone/devices：兼容别名
        path: 'drone/devices',
        redirect: '/drone/drones',
      },
      {
        path: 'drone/drones',
        name: 'DroneList',
        component: () => import('../views/drone/DroneList.vue'),
      },
      {
        path: 'drone/live',
        name: 'DroneLive',
        component: () => import('../views/drone/DroneLive.vue'),
      },
      {
        // 规则 §10.2 的 /drone/control：控制权管理（分控/仲裁）
        path: 'drone/control',
        name: 'DroneControl',
        component: () => import('../views/drone/DroneControl.vue'),
      },
      {
        path: 'drone/routes',
        name: 'DroneRoutes',
        component: () => import('../views/drone/RoutePlanner.vue'),
      },
      {
        // 规则 §10.2 的 /drone/geofence：兼容别名
        path: 'drone/geofence',
        redirect: '/drone/fences',
      },
      {
        path: 'drone/fences',
        name: 'DroneFences',
        component: () => import('../views/drone/FenceManager.vue'),
      },
      {
        path: 'drone/alerts',
        name: 'DroneAlerts',
        component: () => import('../views/drone/DroneAlerts.vue'),
      },
      {
        // 规则 §10.2 的 /drone/replay：轨迹回放
        path: 'drone/replay',
        name: 'DroneReplay',
        component: () => import('../views/drone/DroneReplay.vue'),
      },
      {
        path: 'video-wall',
        name: 'PlayerWall',
        component: () => import('../views/PlayerWall.vue'),
      },
      {
        path: 'video-wall/designer',
        name: 'PlayerWallDesigner',
        component: () => import('../views/PlayerWallDesigner.vue'),
      },
    ],
  },
  {
    path: '/login',
    name: 'Login',
    component: () => import('../views/Login.vue'),
    meta: { public: true },
  },
]

const router = createRouter({
  history: createWebHistory('/'),
  routes,
})

router.beforeEach((to, from, next) => {
  const token = localStorage.getItem('rustcam_access_token')
  if (to.meta.public) {
    if (token && to.name === 'Login') {
      return next({ name: 'Dashboard' })
    }
    next()
  } else if (!token) {
    next({ name: 'Login', query: { redirect: window.location.pathname } })
  } else {
    next()
  }
})

export default router
