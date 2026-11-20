<template>
  <div class="app-aside" :style="{ width: collapsed ? 'var(--sidebar-collapsed)' : 'var(--sidebar-expanded)' }">
    <div class="aside-logo">
      <div class="logo-mark">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 2L2 7l10 5 10-5-10-5z"/>
          <path d="M2 17l10 5 10-5"/>
          <path d="M2 12l10 5 10-5"/>
        </svg>
      </div>
      <span v-if="!collapsed" class="logo-text">RustCam</span>
    </div>

    <div class="nav-menu">
      <el-menu
        :default-active="currentRoute"
        :collapse="collapsed"
        :collapse-transition="false"
        class="nav-menu"
        @select="onMenuSelect"
      >
        <el-menu-item index="/">
          <el-icon><Monitor /></el-icon>
          <template #title><span>系统概览</span></template>
        </el-menu-item>

        <el-sub-menu index="/device-group">
          <template #title>
            <el-icon><VideoCamera /></el-icon>
            <span>设备管理</span>
          </template>
          <el-menu-item index="/devices">
            <el-icon><VideoCamera /></el-icon>
            <template #title><span>设备列表</span></template>
          </el-menu-item>
          <el-menu-item index="/device-map">
            <el-icon><MapLocation /></el-icon>
            <template #title><span>电子地图</span></template>
          </el-menu-item>
          <el-menu-item index="/channels">
            <el-icon><Connection /></el-icon>
            <template #title><span>通道列表</span></template>
          </el-menu-item>
        </el-sub-menu>

        <el-sub-menu index="/drone-group">
          <template #title>
            <el-icon><Promotion /></el-icon>
            <span>移动眼</span>
          </template>
          <el-menu-item index="/drone/drones">
            <el-icon><Aim /></el-icon>
            <template #title><span>无人机</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/adapters">
            <el-icon><Cpu /></el-icon>
            <template #title><span>接入适配器</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/control">
            <el-icon><Unlock /></el-icon>
            <template #title><span>控制权管理</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/live">
            <el-icon><VideoPlay /></el-icon>
            <template #title><span>实时飞行</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/routes">
            <el-icon><Guide /></el-icon>
            <template #title><span>航线规划</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/fences">
            <el-icon><Crop /></el-icon>
            <template #title><span>电子围栏</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/alerts">
            <el-icon><Warning /></el-icon>
            <template #title><span>告警与审计</span></template>
          </el-menu-item>
          <el-menu-item index="/drone/replay">
            <el-icon><RefreshLeft /></el-icon>
            <template #title><span>轨迹回放</span></template>
          </el-menu-item>
        </el-sub-menu>

        <el-sub-menu index="/stream-group">
          <template #title>
            <el-icon><DataLine /></el-icon>
            <span>媒体流</span>
          </template>
          <el-menu-item index="/streams">
            <el-icon><Odometer /></el-icon>
            <template #title><span>流列表</span></template>
          </el-menu-item>
          <el-menu-item index="/sessions">
            <el-icon><Files /></el-icon>
            <template #title><span>实时会话</span></template>
          </el-menu-item>
        </el-sub-menu>

        <el-sub-menu index="/recording-group">
          <template #title>
            <el-icon><VideoPlay /></el-icon>
            <span>录像管理</span>
          </template>
          <el-menu-item index="/recordings">
            <el-icon><Film /></el-icon>
            <template #title><span>录像管理</span></template>
          </el-menu-item>
          <el-menu-item index="/recordings/files">
            <el-icon><Folder /></el-icon>
            <template #title><span>录像文件</span></template>
          </el-menu-item>
          <el-menu-item index="/recording-policies">
            <el-icon><Tickets /></el-icon>
            <template #title><span>录制策略</span></template>
          </el-menu-item>
          <el-menu-item index="/recording-playback">
            <el-icon><RefreshRight /></el-icon>
            <template #title><span>录像回放</span></template>
          </el-menu-item>
        </el-sub-menu>

        <el-menu-item index="/servers">
          <el-icon><Cpu /></el-icon>
          <template #title><span>媒体服务器</span></template>
        </el-menu-item>

        <el-menu-item index="/public">
          <el-icon><Compass /></el-icon>
          <template #title><span>公开直播</span></template>
        </el-menu-item>

        <el-menu-item index="/alarms">
          <el-icon>
            <Bell />
          </el-icon>
          <template #title>
            <span>报警记录</span>
            <el-badge v-if="unprocessedAlarms > 0" :value="unprocessedAlarms > 99 ? '99+' : unprocessedAlarms" type="danger" style="margin-left: 8px" />
          </template>
        </el-menu-item>

        <el-sub-menu index="/wall-group">
          <template #title>
            <el-icon><Grid /></el-icon>
            <span>监控墙</span>
          </template>
          <el-menu-item index="/video-wall">
            <el-icon><Monitor /></el-icon>
            <template #title><span>监控墙</span></template>
          </el-menu-item>
          <el-menu-item index="/video-wall/designer">
            <el-icon><MagicStick /></el-icon>
            <template #title><span>布局设计</span></template>
          </el-menu-item>
        </el-sub-menu>

        <el-sub-menu index="admin-group">
          <template #title>
            <el-icon><User /></el-icon>
            <span>管理</span>
          </template>
          <el-menu-item v-if="canManage" index="/users">
            <el-icon><Avatar /></el-icon>
            <template #title><span>用户管理</span></template>
          </el-menu-item>
          <el-menu-item v-if="canManage" index="/roles">
            <el-icon><UserFilled /></el-icon>
            <template #title><span>角色管理</span></template>
          </el-menu-item>
          <el-menu-item v-if="canManage" index="/permissions">
            <el-icon><Lock /></el-icon>
            <template #title><span>权限管理</span></template>
          </el-menu-item>
          <el-menu-item index="/login-sessions">
            <el-icon><Key /></el-icon>
            <template #title><span>登录会话</span></template>
          </el-menu-item>
          <el-menu-item v-if="canManage" index="/settings">
            <el-icon><Setting /></el-icon>
            <template #title><span>系统设置</span></template>
          </el-menu-item>
        </el-sub-menu>
      </el-menu>
    </div>

    <div class="aside-footer">
      <div class="aside-footer-row" style="justify-content: space-between;">
        <div class="aside-footer-row">
          <span class="status-dot" :class="wsConnected ? 'online' : 'offline'"></span>
          <span v-if="!collapsed" class="ws-indicator">{{ wsConnected ? '在线' : '离线' }}</span>
        </div>
        <el-tooltip v-if="collapsed" content="退出登录" placement="right">
          <el-button text size="small" @click="handleLogout">
            <el-icon><SwitchButton /></el-icon>
          </el-button>
        </el-tooltip>
        <el-button v-else text size="small" @click="handleLogout">
          <el-icon><SwitchButton /></el-icon>
        </el-button>
      </div>
      <div class="aside-footer-row" style="justify-content: flex-end;">
        <el-tooltip v-if="collapsed" content="切换主题" placement="right">
          <el-button text @click="theme.toggle()" size="small">
            <el-icon v-if="theme.isDark"><Sunny /></el-icon>
            <el-icon v-else><Moon /></el-icon>
          </el-button>
        </el-tooltip>
        <el-button v-else text @click="theme.toggle()" size="small">
          <el-icon v-if="theme.isDark"><Sunny /></el-icon>
          <el-icon v-else><Moon /></el-icon>
        </el-button>
        <el-tooltip v-if="collapsed" content="展开" placement="right">
          <el-button text size="small" @click="collapsed = !collapsed">
            <el-icon><DArrowRight /></el-icon>
          </el-button>
        </el-tooltip>
        <el-button v-else text size="small" @click="collapsed = !collapsed">
          <el-icon><DArrowLeft /></el-icon>
        </el-button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessageBox } from 'element-plus'
import { Monitor, VideoCamera, DataLine, VideoPlay, Cpu, Compass, Grid, User, Sunny, Moon, DArrowLeft, DArrowRight, SwitchButton, Bell, Promotion, Key, MapLocation, Connection, Aim, Unlock, Guide, Crop, Warning, RefreshLeft, Odometer, Files, Film, Folder, Tickets, RefreshRight, Avatar, UserFilled, Lock, Setting, MagicStick } from '@element-plus/icons-vue'
import { useThemeStore } from '../../stores/themeStore'
import { useAuthStore } from '../../stores/authStore'
import { useWsConnection } from '../../composables/useWsConnection'
import { getUnprocessedCount } from '../../api/alarms'

const route = useRoute()
const router = useRouter()
const theme = useThemeStore()
const auth = useAuthStore()
const { wsConnected, connect, disconnect } = useWsConnection()

const collapsed = ref(false)
const isAdmin = computed(() => auth.isAdmin)
const canManage = computed(() => auth.isAdmin || !!auth.currentUser?.is_super)
const currentRoute = computed(() => route.path)
const unprocessedAlarms = ref(0)

onMounted(async () => {
  if (auth.isAuthenticated) {
    connect()
    await fetchUnprocessedAlarms()
  }
})

onUnmounted(() => {
  disconnect()
})

function onMenuSelect(index) {
  router.push(index)
}

async function fetchUnprocessedAlarms() {
  try {
    unprocessedAlarms.value = await getUnprocessedCount()
  } catch (e) {
    console.warn('[Sidebar] Failed to fetch unprocessed alarms:', e)
  }
}

async function handleLogout() {
  disconnect()
  await ElMessageBox.confirm('确定要退出登录吗？', '退出确认', { type: 'warning', confirmButtonText: '退出', cancelButtonText: '取消' })
  await auth.logout()
  router.push('/login')
}
</script>
