<!-- @ts-check -->
<template>
  <div class="page-container live-page">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">实时飞行</h1>
        <span class="header-sub">
          {{ selected ? selected.name || selected.device_id : '未选择无人机' }}
          <el-tag v-if="selected" size="small" :type="selected.online ? 'success' : 'info'" style="margin-left: 8px">
            {{ selected.online ? '在线' : '离线' }}
          </el-tag>
        </span>
      </div>
      <div class="page-toolbar">
        <el-radio-group v-model="mapMode" size="small">
          <el-radio-button value="2d">2D</el-radio-button>
          <el-radio-button value="3d">3D</el-radio-button>
        </el-radio-group>
        <el-select
          v-model="routeId"
          size="small"
          clearable
          placeholder="不显示航线"
          style="width: 180px"
        >
          <el-option
            v-for="r in store.routes"
            :key="r.id"
            :label="`${r.name} · ${r.waypoints.length} 点`"
            :value="r.id"
          />
        </el-select>
        <span v-if="selectedRoute" class="route-transit">
          距起点 {{ formatDistance(transitMeters) }}
        </span>
        <el-tag :type="store.wsConnected ? 'success' : 'info'" effect="plain">
          WS {{ store.wsConnected ? '已连接' : '未连接' }}
        </el-tag>
        <el-button :icon="Refresh" @click="refresh">刷新</el-button>
        <el-button :icon="Aim" :disabled="!telemetry" @click="locate">定位</el-button>
      </div>
    </div>

    <div class="live-body">
      <aside class="drone-rail">
        <div class="rail-title">机队</div>
        <div
          v-for="d in store.drones"
          :key="d.device_id"
          class="rail-item"
          :class="{ active: d.device_id === selectedId }"
          @click="select(d.device_id)"
        >
          <span class="status-dot" :class="d.online ? 'online' : 'offline'"></span>
          <div class="rail-main">
            <div class="rail-name">{{ d.name || d.device_id }}</div>
            <div class="rail-meta">
              {{ d.mode || '—' }}
              <span v-if="d.last_telemetry"> · {{ Math.round(d.last_telemetry.battery_pct || 0) }}%</span>
            </div>
          </div>
        </div>
        <el-empty v-if="store.drones.length === 0" description="暂无无人机" :image-size="60" />
      </aside>

      <div class="globe-area">
        <Map2D
          v-if="mapMode === '2d'"
          ref="map2dRef"
          :drones="store.drones"
          :selected-id="selectedId"
          :fences="store.fences"
          :route="liveRoute"
          auto-fit-route
          :draw-mode="pickMode ? 'waypoint' : 'none'"
          :draft-points="pickMode && gotoForm.lat ? [{ lon: gotoForm.lon, lat: gotoForm.lat }] : []"
          @map-click="onMapClick"
          @select="select"
        />
        <CesiumGlobe
          v-else
          ref="globeRef"
          :drones="store.drones"
          :selected-id="selectedId"
          :fences="store.fences"
          :route="liveRoute"
          auto-fit-route
          :draw-mode="pickMode ? 'waypoint' : 'none'"
          :draft-points="pickMode && gotoForm.lat ? [{ lon: gotoForm.lon, lat: gotoForm.lat, alt: gotoForm.alt }] : []"
          @map-click="onMapClick"
        >
          <template #hint>
            <el-tag v-if="pickMode" type="warning" effect="dark">点击地图选择目标点</el-tag>
          </template>
        </CesiumGlobe>
        <el-tag
          v-if="mapMode === '2d' && pickMode"
          class="map-hint"
          type="warning"
          effect="dark"
        >点击地图选择目标点</el-tag>

        <div class="video-card" v-if="selected">
          <div class="card-head">
            <span>机上视频</span>
            <el-select
              v-if="cameraOptions.length > 1"
              v-model="cameraIndex"
              size="small"
              class="video-select"
            >
              <el-option
                v-for="(tag, i) in cameraOptions"
                :key="tag"
                :label="tag"
                :value="i"
              />
            </el-select>
            <el-select
              v-if="channelOptions.length > 1"
              v-model="channelIndex"
              size="small"
              class="video-select"
            >
              <el-option
                v-for="(ch, i) in channelOptions"
                :key="ch.channel_tag"
                :label="ch.name || ch.channel_tag"
                :value="i"
              />
            </el-select>
            <el-tag v-else-if="activeCameraTag" size="small" effect="plain">
              {{ activeCameraTag }}
            </el-tag>
          </div>
          <VideoPlayer
            v-if="videoSrc"
            :live-src="videoSrc"
            type="auto"
            :is-live="true"
            :autoplay="true"
          />
          <div v-else class="video-placeholder">
            <div>未关联可播放视频</div>
            <div class="video-hint">
              在无人机档案中关联摄像头；若通道未推流请先启动播放
            </div>
            <el-button
              v-if="activeCameraTag"
              size="small"
              type="primary"
              plain
              :loading="videoLoading"
              @click="resolveVideo"
            >从平台拉流</el-button>
          </div>
        </div>
      </div>

      <aside class="panel-rail">
        <div class="panel">
          <div class="panel-head">
            <span>遥测</span>
            <span class="panel-time">{{ telemetry ? formatTime(telemetry.ts_ms) : '无数据' }}</span>
          </div>
          <template v-if="telemetry">
            <div class="telemetry-grid">
              <div class="t-item"><span>模式</span><b>{{ telemetry.mode || '—' }}</b></div>
              <div class="t-item"><span>飞控</span><b>{{ (telemetry.autopilot || '—').toUpperCase() }}</b></div>
              <div class="t-item"><span>高度</span><b>{{ telemetry.alt_amsl?.toFixed(1) }} m</b></div>
              <div class="t-item"><span>相对高度</span><b>{{ telemetry.rel_alt?.toFixed(1) }} m</b></div>
              <div class="t-item"><span>速度</span><b>{{ speed?.toFixed(1) }} m/s</b></div>
              <div class="t-item"><span>卫星</span><b>{{ telemetry.satellites || 0 }}</b></div>
              <div class="t-item"><span>经度</span><b class="mono">{{ telemetry.lon?.toFixed(6) }}</b></div>
              <div class="t-item"><span>纬度</span><b class="mono">{{ telemetry.lat?.toFixed(6) }}</b></div>
              <div class="t-item"><span>航向</span><b>{{ (((telemetry.yaw || 0) * 180) / Math.PI).toFixed(0) }}°</b></div>
              <div class="t-item"><span>链路</span><b>{{ telemetry.link_quality || 0 }}%</b></div>
            </div>
            <div class="battery-block">
              <div class="battery-row">
                <span>电池</span>
                <b>{{ Math.round(telemetry.battery_pct || 0) }}% · {{ (telemetry.voltage || 0).toFixed(1) }}V</b>
              </div>
              <el-progress
                :percentage="Math.round(telemetry.battery_pct || 0)"
                :status="telemetry.battery_pct <= 20 ? 'exception' : telemetry.battery_pct <= 40 ? 'warning' : 'success'"
                :stroke-width="8"
              />
            </div>
          </template>
          <el-empty v-else description="尚未收到遥测" :image-size="60" />
        </div>

        <div class="panel">
          <div class="panel-head">
            <span>控制权</span>
            <el-tag
              v-if="control?.held"
              :type="control.granted ? 'success' : 'warning'"
              size="small"
            >
              {{ control.granted ? '你持有' : `${control.holder} 持有` }}
            </el-tag>
            <el-tag v-else type="info" size="small">未持有</el-tag>
          </div>
          <div class="control-row">
            <el-button
              size="small"
              type="primary"
              :disabled="!selected || control?.held"
              @click="acquire"
            >申请控制</el-button>
            <el-button size="small" :disabled="!control?.granted" @click="release">释放</el-button>
          </div>
          <div class="lease-row">
            <span>租约时长</span>
            <el-select v-model="leaseSeconds" size="small" :disabled="control?.granted" style="width: 110px">
              <el-option
                v-for="opt in leaseOptions"
                :key="opt.value"
                :label="opt.label"
                :value="opt.value"
              />
            </el-select>
          </div>
          <div class="control-hint" v-if="control?.held">
            优先级 P{{ control.priority }} · 剩余租约 {{ leaseRemain }}s
            <span v-if="control.expire_at_ms"> · {{ formatTime(control.expire_at_ms) }} 到期</span>
          </div>
          <div class="control-hint" v-else>飞行指令前需要先申请控制权</div>
        </div>

        <div class="panel">
          <div class="panel-head"><span>飞行指令</span></div>
          <div class="cmd-grid">
            <el-button type="success" :disabled="!canCommand" @click="takeoff">起飞</el-button>
            <el-button type="warning" :disabled="!canCommand" @click="land">降落</el-button>
            <el-button type="danger" :disabled="!canCommand" @click="rtl">返航</el-button>
            <el-button :disabled="!canCommand" @click="togglePick">
              {{ pickMode ? '取消选点' : '地图选点' }}
            </el-button>
          </div>
          <div class="route-offset-row">
            <span>横向偏移</span>
            <el-input-number
              v-model="routeOffset"
              :min="-200"
              :max="200"
              :step="10"
              size="small"
              controls-position="right"
              style="width: 110px"
            />
            <span>m（正右负左）</span>
          </div>
          <el-button
            class="route-exec-btn"
            type="primary"
            plain
            :disabled="!canCommand || !selectedRoute"
            @click="executeSelectedRoute"
          >
            {{ selectedRoute ? `执行航线：${selectedRoute.name}` : '执行航线（请先选择航线）' }}
          </el-button>
          <div class="goto-form">
            <el-input-number v-model="gotoForm.lat" :precision="6" :step="0.0001" controls-position="right" placeholder="纬度" />
            <el-input-number v-model="gotoForm.lon" :precision="6" :step="0.0001" controls-position="right" placeholder="经度" />
            <el-input-number v-model="gotoForm.alt" :min="1" :max="500" controls-position="right" placeholder="高度" />
            <el-button type="primary" :disabled="!canCommand" @click="goto">定点飞行</el-button>
          </div>
          <div class="yaw-form">
            <el-input-number v-model="bodyYaw" :min="0" :max="359" :step="15" controls-position="right" />
            <el-button :disabled="!canCommand" @click="turnBody">机体转向</el-button>
            <span>°（绝对航向）</span>
          </div>

          <div class="panel-subhead">云台</div>
          <div class="gimbal-form">
            <el-input-number v-model="gimbal.pitch" :min="-90" :max="90" controls-position="right" />
            <el-input-number v-model="gimbal.yaw" :min="-180" :max="180" controls-position="right" />
            <el-button :disabled="!canCommand" @click="gimbalMove">云台转向</el-button>
          </div>

          <div class="panel-subhead">相机</div>
          <div class="cmd-grid">
            <el-button @click="camera('photo')">拍照</el-button>
            <el-button @click="camera('video_start')">录像</el-button>
            <el-button @click="camera('video_stop')">停止录像</el-button>
          </div>
        </div>

        <div class="panel">
          <div class="panel-head"><span>最近指令</span></div>
          <div v-if="recentCommands.length === 0" class="empty-hint">暂无指令</div>
          <div v-for="c in recentCommands" :key="c.cmd_id" class="cmd-item">
            <span class="cmd-kind">{{ c.kind || c.cmd_id?.slice(0, 8) }}</span>
            <el-tag size="small" :type="statusType(c.status)">{{ c.status }}</el-tag>
            <span class="cmd-msg">{{ c.message }}</span>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Aim, Refresh } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import { getChannelPlayLinks, getDeviceChannels } from '../../api/channels'
import { DEFAULT_LEASE_SECONDS, LEASE_OPTIONS } from '../../utils/droneLease'
import CesiumGlobe from '../../components/drone/CesiumGlobe.vue'
import Map2D from '../../components/drone/Map2D.vue'
import VideoPlayer from '../../components/VideoPlayer.vue'

const store = useDroneStore()
const route = useRoute()
const globeRef = ref(null)
const map2dRef = ref(null)
const mapMode = ref('3d')
const selectedId = ref('')
/** 当前地图上显示的计划航线 id；空表示不显示。 */
const routeId = ref('')
const pickMode = ref(false)
const videoLoading = ref(false)
const resolvedVideo = ref('')
/** 关联摄像头与通道的选择下标。 */
const cameraIndex = ref(0)
const channelIndex = ref(0)
const channelOptions = ref([])
const gotoForm = reactive({ lat: 31.2304, lon: 121.4737, alt: 30 })
const gimbal = reactive({ pitch: -30, yaw: 0 })
/** 机体绝对航向（度）。 */
const bodyYaw = ref(0)
/** 申请控制权时的租约时长（秒）；续约沿用同一时长。 */
const leaseSeconds = ref(DEFAULT_LEASE_SECONDS)
const leaseOptions = LEASE_OPTIONS

const selected = computed(() =>
  store.drones.find(d => d.device_id === selectedId.value) || null
)
/** 可选摄像头：优先用关联的摄像头，旧档案回退到 device_tag。 */
const cameraOptions = computed(() => {
  const tags = selected.value?.camera_device_tags || []
  if (tags.length > 0) return tags
  return selected.value?.device_tag ? [selected.value.device_tag] : []
})
const activeCameraTag = computed(() => cameraOptions.value[cameraIndex.value] || '')
const activeChannel = computed(() => channelOptions.value[channelIndex.value] || null)
const telemetry = computed(() =>
  telemetryFor(selectedId.value)
)
const control = computed(() =>
  store.getControl(selectedId.value, 'flight')
)
/** 实时地图上叠加的“意图航线”：当前无人机已保存的航线。 */
const liveRoute = computed(() => {
  const r = store.routes.find((x) => x.id === routeId.value)
  if (!r) return []
  if (r.device_id && r.device_id !== selectedId.value) return []
  return r.waypoints || []
})
/** 本次执行航线的横向偏移（米，正=右，负=左）。 */
const routeOffset = ref(0)

const END_ACTION_LABELS = {
  hold: '在终点悬停',
  rtl: '自动返航',
  land: '在终点降落',
}
const selectedRoute = computed(
  () => store.routes.find((x) => x.id === routeId.value) || null
)
/** 当前位置到航线起点的转场距离（米）。 */
const transitMeters = computed(() => {
  const t = telemetry.value
  const w = selectedRoute.value?.waypoints?.[0]
  if (!t || !w || !Number(t.lat) || !Number(t.lon)) return 0
  return haversineMeters(Number(t.lat), Number(t.lon), Number(w.lat), Number(w.lon))
})
const canCommand = computed(() => !!selected.value && selected.value.online)
const now = ref(Date.now())
const leaseRemain = computed(() => {
  // 显式依赖 now，租约倒计时才会每秒刷新（规则 §10.7）。
  now.value
  const c = control.value
  if (!c?.held || !c.expire_at_ms) return 0
  return Math.max(0, Math.round((c.expire_at_ms - Date.now()) / 1000))
})
let leaseTimer = null
onMounted(() => {
  leaseTimer = setInterval(() => {
    now.value = Date.now()
  }, 1000)
})
onUnmounted(() => {
  if (leaseTimer) clearInterval(leaseTimer)
})
const videoSrc = computed(() => resolvedVideo.value)
const speed = computed(() => {
  const t = telemetry.value
  if (!t) return 0
  return Math.sqrt((t.vx || 0) ** 2 + (t.vy || 0) ** 2 + (t.vz || 0) ** 2)
})
const recentCommands = computed(() =>
  Object.values(store.commands)
    .filter(c => !c.device_id || c.device_id === selectedId.value)
    .sort((a, b) => (b.ts_ms || 0) - (a.ts_ms || 0))
    .slice(0, 8)
)

watch(() => store.drones, () => {
  if (!selectedId.value && store.drones.length > 0) {
    select(route.query.device || store.drones[0].device_id)
  }
}, { immediate: true })

watch(() => route.query.device, (id) => {
  if (id) select(id)
})

function telemetryFor(id) {
  if (!id) return null
  return store.telemetry[id] || store.drones.find(d => d.device_id === id)?.last_telemetry || null
}

function select(id) {
  selectedId.value = id
  resolvedVideo.value = ''
  cameraIndex.value = 0
  channelIndex.value = 0
  channelOptions.value = []
  const t = telemetryFor(id)
  if (t && t.lat && t.lon) {
    gotoForm.lat = t.lat
    gotoForm.lon = t.lon
    gotoForm.alt = Math.max(10, Math.round(t.rel_alt || 30))
  }
  store.fetchControl(id, 'flight')
  loadRoutes(id)
}

/** 拉取当前无人机的航线；保留已选航线，失效时回退到第一条。 */
async function loadRoutes(id) {
  if (!id) return
  try {
    await store.fetchRoutes(id)
  } catch (e) {
    return
  }
  if (!store.routes.some((r) => r.id === routeId.value)) {
    routeId.value = store.routes[0]?.id || ''
  }
}

async function loadCameraChannels() {
  const cameraTag = activeCameraTag.value
  if (!cameraTag) {
    channelOptions.value = []
    return
  }
  try {
    const list = await getDeviceChannels(cameraTag)
    channelOptions.value = list || []
    channelIndex.value = 0
    if (channelOptions.value.length > 0) {
      await resolveVideo()
    }
  } catch (e) {
    channelOptions.value = []
  }
}

watch([cameraIndex, selectedId], async () => {
  channelIndex.value = 0
  channelOptions.value = []
  resolvedVideo.value = ''
  await loadCameraChannels()
})

watch(channelIndex, () => {
  resolvedVideo.value = ''
  if (activeChannel.value) resolveVideo()
})

async function resolveVideo() {
  const cameraTag = activeCameraTag.value
  if (!cameraTag) return
  const channelTag = activeChannel.value?.channel_tag
    || selected.value?.channel_tag
    || cameraTag
  videoLoading.value = true
  try {
    const data = await getChannelPlayLinks(cameraTag, channelTag)
    const url = data?.flv || data?.web_flv || data?.hls || data?.webrtc
    if (!url) {
      ElMessage.warning('平台未返回播放地址，请确认通道已推流')
      return
    }
    resolvedVideo.value = url
  } catch (e) {
    ElMessage.error('获取播放地址失败：' + (e.message || e))
  } finally {
    videoLoading.value = false
  }
}

function refresh() {
  store.fetchDrones()
  if (selectedId.value) {
    loadRoutes(selectedId.value)
    loadCameraChannels()
  }
}

function locate() {
  const t = telemetry.value
  if (!t || !t.lat || !t.lon) return
  if (mapMode.value === '2d') map2dRef.value?.flyTo(t.lon, t.lat, 15)
  else globeRef.value?.flyTo(t.lon, t.lat, 300000)
}

function togglePick() {
  pickMode.value = !pickMode.value
}

function onMapClick({ lon, lat }) {
  if (!pickMode.value) return
  gotoForm.lon = Number(lon.toFixed(6))
  gotoForm.lat = Number(lat.toFixed(6))
  pickMode.value = false
  ElMessage.success('目标点已选择')
}

async function ensureControl() {
  if (control.value?.granted) return true
  const c = await store.acquireControl(selectedId.value, 'flight', 10, leaseSeconds.value)
  return !!c?.granted
}

async function maybeCommand(kind, params, confirmText) {
  if (!canCommand.value) {
    ElMessage.warning('无人机不在线')
    return false
  }
  if (!(await ensureControl())) return false
  if (confirmText) {
    try {
      await ElMessageBox.confirm(confirmText, '指令确认', {
        type: 'warning',
        confirmButtonText: '执行',
        cancelButtonText: '取消',
      })
    } catch (e) {
      return false
    }
  }
  const reply = await store.sendCommand(selectedId.value, kind, params)
  return !!reply
}

/**
 * 二次确认文案：必须包含目标点、高度与返航点（规则 §10.7）。
 */
function confirmTextFor(kind, detail) {
  const t = telemetry.value
  const home = t && t.home_lat
    ? `返航点 ${Number(t.home_lat).toFixed(6)}, ${Number(t.home_lon).toFixed(6)}（${Math.round(Number(t.home_alt || 0))}m）`
    : '返航点未知'
  return `${detail}\n高度：${gotoForm.alt}m（相对起飞点）\n${home}\n确认执行 ${kind} 指令？`
}

function takeoff() {
  ElMessageBox.prompt('起飞高度（米，相对 home）', '起飞', {
    inputValue: String(gotoForm.alt || 30),
    inputPattern: /^\d+(\.\d+)?$/,
    inputErrorMessage: '请输入数字',
  }).then(async ({ value }) => {
    await maybeCommand('takeoff', { alt: value }, confirmTextFor('起飞', `起飞到 ${value}m`))
  }).catch(() => {})
}

function land() {
  return maybeCommand('land', {}, confirmTextFor('降落', '让无人机降落'))
}

function rtl() {
  return maybeCommand('rtl', {}, confirmTextFor('返航', '让无人机返航'))
}

function goto() {
  return maybeCommand(
    'goto',
    { lat: String(gotoForm.lat), lon: String(gotoForm.lon), alt: String(gotoForm.alt) },
    confirmTextFor('定点飞行', `飞往 ${gotoForm.lat}, ${gotoForm.lon}`)
  )
}

/**
 * 触发一次航线任务。执行、起飞、租约维护、返航都由 drone-service 负责，
 * 前端只做二次确认并展示结果。
 */
async function executeSelectedRoute() {
  const r = selectedRoute.value
  if (!r) {
    ElMessage.warning('请先在顶部选择航线')
    return
  }
  if (!canCommand.value) {
    ElMessage.warning('无人机不在线')
    return
  }
  if (!(await ensureControl())) return
  try {
    await ElMessageBox.confirm(
      `执行航线「${r.name}」？共 ${r.waypoints.length} 个航点。\n` +
        `本次仅执行当前选中的无人机（${selected.value?.name || selectedId.value}）。\n` +
        `当前位置距起点 ${formatDistance(transitMeters.value)}，任务会先飞往起点，再按航点执行；` +
        `横向偏移 ${routeOffset.value}m；` +
        `结束后${END_ACTION_LABELS[r.end_action] || END_ACTION_LABELS.hold}。` +
        '（必要时服务端会自动起飞并延长租约）',
      '执行航线',
      { type: 'warning', confirmButtonText: '执行', cancelButtonText: '取消' }
    )
  } catch (e) {
    return
  }
  try {
    await store.executeRoute(r.id, selectedId.value, routeOffset.value)
    // 服务端会把租约延长到覆盖整条任务，刷新一次状态让剩余时长立即同步。
    store.fetchControl(selectedId.value, 'flight')
  } catch (e) {
    ElMessage.error(e?.response?.data?.message || e?.message || '航线执行失败')
  }
}

function haversineMeters(lat1, lon1, lat2, lon2) {
  const R = 6371000
  const rad = Math.PI / 180
  const dLat = (lat2 - lat1) * rad
  const dLon = (lon2 - lon1) * rad
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(lat1 * rad) * Math.cos(lat2 * rad) * Math.sin(dLon / 2) ** 2
  return 2 * R * Math.asin(Math.min(1, Math.sqrt(h)))
}

/** @param {number} meters */
function formatDistance(meters) {
  const m = Number(meters) || 0
  return m >= 1000 ? `${(m / 1000).toFixed(2)} km` : `${Math.round(m)} m`
}

async function turnBody() {
  await maybeCommand(
    'yaw',
    { angle: String(bodyYaw.value), speed: '30' },
    `机体转向到 ${bodyYaw.value}°？`
  )
}

async function gimbalMove() {
  if (!canCommand.value) {
    ElMessage.warning('无人机不在线')
    return
  }
  if (!(await ensureControlGimbal())) return
  try {
    await ElMessageBox.confirm(
      `云台转到 pitch ${gimbal.pitch}° / yaw ${gimbal.yaw}°？`,
      '指令确认',
      { type: 'warning', confirmButtonText: '执行', cancelButtonText: '取消' }
    )
  } catch (e) {
    return
  }
  await store.sendCommand(selectedId.value, 'gimbal', {
    pitch: String(gimbal.pitch),
    yaw: String(gimbal.yaw),
  })
}

async function ensureControlGimbal() {
  if (store.getControl(selectedId.value, 'gimbal')?.granted) return true
  const c = await store.acquireControl(selectedId.value, 'gimbal', 5, leaseSeconds.value)
  return !!c?.granted
}

async function camera(action) {
  if (!canCommand.value) {
    ElMessage.warning('无人机不在线')
    return
  }
  if (!(await ensureControlGimbal())) return
  try {
    await ElMessageBox.confirm(`确认执行相机动作 ${action}？`, '指令确认', {
      type: 'warning',
      confirmButtonText: '执行',
      cancelButtonText: '取消',
    })
  } catch (e) {
    return
  }
  await store.sendCommand(selectedId.value, 'camera', { action })
}

function acquire() {
  store.acquireControl(selectedId.value, 'flight', 10, leaseSeconds.value)
}

function release() {
  store.releaseControl(selectedId.value, 'flight')
}

function formatTime(ms) {
  if (!ms) return '—'
  return new Date(ms).toLocaleTimeString('zh-CN', { hour12: false })
}

function statusType(status) {
  if (['accepted', 'succeeded', 'sent'].includes(status)) return 'success'
  if (['failed', 'rejected', 'timeout'].includes(status)) return 'danger'
  return 'warning'
}

onMounted(async () => {
  await Promise.all([store.fetchDrones(), store.fetchFences()])
  store.connectWs()
  const id = route.query.device || store.drones[0]?.device_id
  if (id) select(id)
})
</script>

<style scoped>
.live-page {
  background: var(--bg-base);
}
.header-left {
  display: flex;
  align-items: baseline;
  gap: 12px;
}
.header-sub {
  font-size: 12px;
  color: var(--text-muted);
}
.live-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr) 340px;
  gap: 10px;
  padding: 10px;
}
.drone-rail,
.panel-rail {
  min-height: 0;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-surface);
}
.drone-rail {
  padding: 10px;
}
.rail-title {
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 8px;
}
.rail-item {
  display: flex;
  gap: 10px;
  padding: 10px;
  border-radius: 8px;
  cursor: pointer;
  border: 1px solid transparent;
  margin-bottom: 6px;
}
.rail-item:hover {
  background: var(--bg-elevated);
}
.rail-item.active {
  border-color: var(--color-accent);
  background: var(--bg-elevated);
}
.rail-main {
  min-width: 0;
}
.rail-name {
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.rail-meta {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  margin-top: 2px;
}
.globe-area {
  position: relative;
  min-height: 0;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid var(--border);
}
.globe-area :deep(.cesium-globe),
.globe-area :deep(.drone-map2d) {
  height: 100%;
}
.map-hint {
  position: absolute;
  top: 12px;
  left: 12px;
  z-index: 5;
}
.route-transit {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
}
.video-card {
  position: absolute;
  right: 12px;
  bottom: 12px;
  width: 300px;
  border-radius: 10px;
  overflow: hidden;
  background: rgba(8, 15, 30, 0.92);
  border: 1px solid var(--border);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
}
.card-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  padding: 8px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  border-bottom: 1px solid var(--border);
}
.video-select {
  width: 130px;
}
.video-card :deep(.video-player) {
  height: 170px;
}
.video-placeholder {
  height: 170px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  color: var(--text-muted);
  font-size: 12px;
}
.video-hint {
  font-size: 11px;
  max-width: 220px;
  text-align: center;
  line-height: 1.5;
}
.panel-rail {
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.panel {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px;
  background: var(--bg-base);
}
.panel-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 8px;
}
.panel-time {
  font-family: var(--font-mono);
  font-size: 10px;
  color: var(--text-muted);
}
.telemetry-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px 10px;
}
.t-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 11px;
  color: var(--text-muted);
}
.t-item b {
  font-size: 12px;
  color: var(--text-primary);
  font-family: var(--font-mono);
}
.mono {
  font-family: var(--font-mono);
}
.battery-block {
  margin-top: 10px;
}
.battery-row {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-muted);
  margin-bottom: 4px;
}
.control-row {
  display: flex;
  gap: 8px;
}
.lease-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-muted);
}
.control-hint {
  margin-top: 6px;
  font-size: 11px;
  color: var(--text-muted);
}
.cmd-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}
.route-exec-btn {
  width: 100%;
  margin-top: 8px;
}
.route-offset-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  font-size: 11px;
  color: var(--text-muted);
}
.goto-form,
.gimbal-form {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-top: 8px;
}
.goto-form :deep(.el-input-number),
.gimbal-form :deep(.el-input-number) {
  width: 100%;
}
.yaw-form {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
}
.yaw-form :deep(.el-input-number) {
  flex: 1;
}
.yaw-form span {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}
.panel-subhead {
  font-size: 11px;
  color: var(--text-muted);
  margin: 10px 0 2px;
}
.cmd-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  padding: 4px 0;
  border-bottom: 1px dashed var(--border);
}
.cmd-kind {
  font-family: var(--font-mono);
  min-width: 48px;
}
.cmd-msg {
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.empty-hint {
  font-size: 11px;
  color: var(--text-muted);
}
@media (max-width: 1280px) {
  .live-body {
    grid-template-columns: 180px minmax(0, 1fr) 300px;
  }
  .video-card {
    width: 240px;
  }
}
</style>
