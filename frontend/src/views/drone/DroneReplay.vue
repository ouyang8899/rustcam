<!-- @ts-check -->
<template>
  <div class="page-container replay-page">
    <div class="page-header replay-header">
      <div class="header-left">
        <h1 class="page-title">轨迹回放</h1>
        <span class="header-sub">
          {{ selected ? selected.name || selected.device_id : '未选择无人机' }}
          <el-tag
            v-if="selected"
            size="small"
            :type="selected.online ? 'success' : 'info'"
            style="margin-left: 8px"
          >
            {{ selected.online ? '在线' : '离线' }}
          </el-tag>
        </span>
      </div>
      <div class="page-toolbar replay-toolbar">
        <el-select
          v-model="deviceId"
          placeholder="选择无人机"
          style="width: 190px"
          :disabled="store.drones.length === 0"
          @change="onDeviceChange"
        >
          <el-option
            v-for="d in store.drones"
            :key="d.device_id"
            :label="d.name || d.device_id"
            :value="d.device_id"
          />
        </el-select>
        <el-date-picker
          v-model="range"
          type="datetimerange"
          range-separator="至"
          start-placeholder="开始时间"
          end-placeholder="结束时间"
          :shortcuts="shortcuts"
          style="width: 350px"
          @change="onRangeChange"
        />
        <el-button type="primary" :icon="Search" :loading="loading" @click="load">查询</el-button>
      </div>
    </div>

    <div class="replay-body">
      <section class="stage">
        <div class="map-wrap">
          <Map2D
            v-if="mapMode === '2d'"
            ref="map2dRef"
            :drones="replayDrones"
            :track="track"
            :track-progress="progress"
          />
          <CesiumGlobe
            v-else
            ref="globeRef"
            :drones="replayDrones"
            :track="track"
            :center="globeCenter"
          />
          <div class="map-mode">
            <el-radio-group v-model="mapMode" size="small">
              <el-radio-button value="2d">2D</el-radio-button>
              <el-radio-button value="3d">3D</el-radio-button>
            </el-radio-group>
          </div>
          <div v-if="loading" class="map-overlay">
            <el-icon class="is-loading"><Loading /></el-icon>
            <span>正在查询遥测历史…</span>
          </div>
          <div v-else-if="samples.length === 0" class="map-overlay empty">
            <el-icon><DataLine /></el-icon>
            <span>{{ store.drones.length === 0 ? '暂无无人机档案' : '该时间窗内没有遥测历史' }}</span>
            <span class="overlay-hint">
              {{
                store.drones.length === 0
                  ? '请先在「无人机」页面接入设备'
                  : '换一个时间段，或确认适配器正在上报遥测'
              }}
            </span>
          </div>
        </div>

        <div class="timeline-card">
          <div class="timeline-top">
            <div class="transport">
              <el-button
                type="primary"
                circle
                :icon="playing ? VideoPause : VideoPlay"
                :disabled="samples.length < 2"
                :title="playing ? '暂停' : '播放'"
                @click="togglePlay"
              />
              <el-button
                circle
                :icon="DArrowLeft"
                :disabled="samples.length < 2"
                title="上一帧"
                @click="jump(-1)"
              />
              <el-button
                circle
                :icon="DArrowRight"
                :disabled="samples.length < 2"
                title="下一帧"
                @click="jump(1)"
              />
              <el-button
                circle
                :icon="RefreshLeft"
                :disabled="samples.length === 0"
                title="回到起点"
                @click="restart"
              />
            </div>
            <div class="clock">
              <span class="clock-current">{{ current ? formatClock(current.ts_ms) : '--:--:--' }}</span>
              <span class="clock-sep">/</span>
              <span class="clock-total">{{ samples.length ? formatClock(t1) : '--:--:--' }}</span>
              <el-tag v-if="samples.length" size="small" effect="plain" class="progress-tag">
                {{ Math.round(progress * 100) }}%
              </el-tag>
            </div>
            <div class="speed-group">
              <span class="speed-label">倍速</span>
              <el-radio-group v-model="speed" size="small">
                <el-radio-button :value="0.5">0.5×</el-radio-button>
                <el-radio-button :value="1">1×</el-radio-button>
                <el-radio-button :value="2">2×</el-radio-button>
                <el-radio-button :value="4">4×</el-radio-button>
                <el-radio-button :value="8">8×</el-radio-button>
              </el-radio-group>
            </div>
          </div>
          <div class="timeline-slider">
            <el-slider
              v-model="sliderSec"
              :min="0"
              :max="sliderMax"
              :step="0.1"
              :format-tooltip="formatSec"
              :disabled="samples.length < 2"
            />
            <div class="timeline-ticks">
              <span>{{ samples.length ? formatDateTime(t0) : '' }}</span>
              <span>{{ samples.length ? formatDateTime(t1) : '' }}</span>
            </div>
          </div>
        </div>
      </section>

      <aside class="side-rail">
        <div class="panel">
          <div class="panel-head">
            <span>轨迹概况</span>
            <el-tag v-if="samples.length" size="small" effect="plain">{{ samples.length }} 帧</el-tag>
          </div>
          <div class="stat-grid">
            <div class="stat"><span>时长</span><b>{{ formatDuration(durationMs) }}</b></div>
            <div class="stat"><span>里程</span><b>{{ formatDistance(totalDistance) }}</b></div>
            <div class="stat"><span>最高相对高度</span><b>{{ maxRelAlt.toFixed(1) }} m</b></div>
            <div class="stat"><span>最大速度</span><b>{{ maxSpeed.toFixed(1) }} m/s</b></div>
          </div>
        </div>

        <div class="panel">
          <div class="panel-head">
            <span>当前遥测</span>
            <span class="panel-time">{{ current ? formatClock(current.ts_ms) : '无数据' }}</span>
          </div>
          <template v-if="current">
            <div class="telemetry-grid">
              <div class="t-item"><span>模式</span><b>{{ current.mode || '—' }}</b></div>
              <div class="t-item"><span>解锁</span><b>{{ current.armed ? '已解锁' : '未解锁' }}</b></div>
              <div class="t-item"><span>相对高度</span><b>{{ formatNum(current.rel_alt, 1) }} m</b></div>
              <div class="t-item"><span>海拔</span><b>{{ formatNum(current.alt_amsl, 1) }} m</b></div>
              <div class="t-item"><span>速度</span><b>{{ currentSpeed.toFixed(1) }} m/s</b></div>
              <div class="t-item"><span>垂直速度</span><b>{{ formatNum(current.vz, 2) }} m/s</b></div>
              <div class="t-item"><span>航向</span><b>{{ currentYaw.toFixed(0) }}°</b></div>
              <div class="t-item"><span>卫星</span><b>{{ current.satellites || 0 }}</b></div>
              <div class="t-item"><span>链路</span><b>{{ Math.round(current.link_quality || 0) }}%</b></div>
              <div class="t-item"><span>飞控</span><b>{{ (current.autopilot || '—').toUpperCase() }}</b></div>
              <div class="t-item wide">
                <span>经纬度</span>
                <b class="mono">{{ current.lon.toFixed(6) }}, {{ current.lat.toFixed(6) }}</b>
              </div>
            </div>
            <div class="battery-block">
              <div class="battery-row">
                <span>电池</span>
                <b>{{ Math.round(current.battery_pct || 0) }}% · {{ (current.voltage || 0).toFixed(1) }}V</b>
              </div>
              <el-progress
                :percentage="Math.round(current.battery_pct || 0)"
                :status="batteryStatus"
                :stroke-width="8"
              />
            </div>
          </template>
          <el-empty v-else description="暂无遥测" :image-size="52" />
        </div>

        <div class="panel">
          <div class="panel-head"><span>高度 / 速度</span></div>
          <v-chart v-if="chartSamples.length" class="profile-chart" :option="profileOption" autoresize />
          <el-empty v-else description="暂无曲线" :image-size="52" />
        </div>
      </aside>
    </div>
  </div>
</template>

<script setup>
// @ts-check
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import {
  DArrowLeft,
  DArrowRight,
  DataLine,
  Loading,
  RefreshLeft,
  Search,
  VideoPause,
  VideoPlay,
} from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import Map2D from '../../components/drone/Map2D.vue'
import CesiumGlobe from '../../components/drone/CesiumGlobe.vue'

const store = useDroneStore()
const route = useRoute()
const router = useRouter()

const map2dRef = ref(null)
const globeRef = ref(null)
const mapMode = ref('2d')
const deviceId = ref('')
/** @type {import('vue').Ref<Date[]>} */
const range = ref([])
const loading = ref(false)
const playing = ref(false)
const speed = ref(1)
/** 当前回放时间戳（ms），在样本之间按进度线性插值。 */
const currentTs = ref(0)
/** @type {import('vue').Ref<any[]>} */
const samples = ref([])

let raf = 0
let lastFrame = 0
let loadSeq = 0

const shortcuts = [
  { text: '最近 5 分钟', value: () => [new Date(Date.now() - 5 * 60 * 1000), new Date()] },
  { text: '最近 15 分钟', value: () => [new Date(Date.now() - 15 * 60 * 1000), new Date()] },
  { text: '最近 1 小时', value: () => [new Date(Date.now() - 60 * 60 * 1000), new Date()] },
  {
    text: '今天',
    value: () => {
      const start = new Date()
      start.setHours(0, 0, 0, 0)
      return [start, new Date()]
    },
  },
]

const selected = computed(() => store.drones.find((d) => d.device_id === deviceId.value) || null)
const t0 = computed(() => (samples.value.length ? samples.value[0].ts_ms : 0))
const t1 = computed(() =>
  samples.value.length ? samples.value[samples.value.length - 1].ts_ms : 0
)
const durationMs = computed(() => Math.max(0, t1.value - t0.value))
const durationSec = computed(() => durationMs.value / 1000)
const sliderMax = computed(() => Math.max(durationSec.value, 0.001))
const progress = computed(() => {
  if (!samples.value.length || durationMs.value <= 0) return 0
  return Math.min(1, Math.max(0, (currentTs.value - t0.value) / durationMs.value))
})

/** 回放用的轨迹点，供 2D/3D 地图绘制。 */
const track = computed(() =>
  samples.value.map((t) => ({ lat: t.lat, lon: t.lon, alt: t.alt_amsl }))
)

/** 把当前插值样本伪装成一台“无人机”，复用地图上的机标与姿态渲染。 */
const replayDrones = computed(() => {
  const t = current.value
  if (!t) return []
  return [
    {
      device_id: deviceId.value || '__replay__',
      name: selected.value?.name || deviceId.value || '回放',
      online: true,
      armed: !!t.armed,
      last_telemetry: t,
    },
  ]
})

const current = computed(() => sampleAt(currentTs.value))
const currentSpeed = computed(() => (current.value ? speedOf(current.value) : 0))
const currentYaw = computed(() => {
  if (!current.value) return 0
  const deg = ((Number(current.value.yaw) || 0) * 180) / Math.PI
  return ((deg % 360) + 360) % 360
})
const batteryStatus = computed(() => {
  const b = current.value?.battery_pct ?? 100
  if (b <= 20) return 'exception'
  if (b <= 40) return 'warning'
  return 'success'
})

const totalDistance = computed(() => {
  const list = samples.value
  let sum = 0
  for (let i = 1; i < list.length; i++) sum += haversine(list[i - 1], list[i])
  return sum
})
const maxRelAlt = computed(() =>
  samples.value.reduce((max, t) => Math.max(max, Number(t.rel_alt) || 0), 0)
)
const maxSpeed = computed(() => samples.value.reduce((max, t) => Math.max(max, speedOf(t)), 0))

/** 时间轴滑块的秒数视图，拖动时暂停并跳转。 */
const sliderSec = computed({
  get: () => Math.max(0, (currentTs.value - t0.value) / 1000),
  set: (value) => {
    if (!samples.value.length) return
    pause()
    currentTs.value = t0.value + Number(value) * 1000
  },
})

/** 3D 视角中心：轨迹包围盒中心，高度随轨迹跨度变化。 */
const globeCenter = computed(() => {
  const m = trackMetrics()
  return { lon: m.lon, lat: m.lat, alt: Math.max(1200, m.span * 2) }
})

const chartSamples = computed(() => {
  const list = samples.value
  if (list.length <= 240) return list
  const step = Math.ceil(list.length / 240)
  const out = list.filter((_, index) => index % step === 0)
  if (out[out.length - 1] !== list[list.length - 1]) out.push(list[list.length - 1])
  return out
})

/** 图表游标按整秒更新，避免逐帧重绘 ECharts。 */
const cursorSec = computed(() => Math.max(0, Math.round((currentTs.value - t0.value) / 1000)))

const profileOption = computed(() => {
  const data = chartSamples.value
  const base = t0.value
  const maxSec = Math.max(1, durationSec.value)
  const timeOf = (t) => Math.max(0, (t.ts_ms - base) / 1000)
  return {
    animation: false,
    grid: { left: 42, right: 42, top: 28, bottom: 24 },
    legend: {
      top: 0,
      right: 0,
      itemWidth: 10,
      itemHeight: 6,
      textStyle: { fontSize: 10, color: '#909399' },
    },
    tooltip: { trigger: 'axis', confine: true, textStyle: { fontSize: 11 } },
    xAxis: {
      type: 'value',
      min: 0,
      max: maxSec,
      axisLabel: {
        fontSize: 10,
        color: '#909399',
        formatter: (v) => formatDuration(Number(v) * 1000),
      },
      axisLine: { lineStyle: { color: 'rgba(148,163,184,0.35)' } },
      splitLine: { show: false },
    },
    yAxis: [
      {
        type: 'value',
        name: 'm',
        nameTextStyle: { fontSize: 10, color: '#909399' },
        axisLabel: { fontSize: 10, color: '#909399' },
        splitLine: { lineStyle: { color: 'rgba(148,163,184,0.18)' } },
      },
      {
        type: 'value',
        name: 'm/s',
        nameTextStyle: { fontSize: 10, color: '#909399' },
        axisLabel: { fontSize: 10, color: '#909399' },
        splitLine: { show: false },
      },
    ],
    series: [
      {
        name: '相对高度',
        type: 'line',
        showSymbol: false,
        data: data.map((t) => [timeOf(t), Number(t.rel_alt) || 0]),
        lineStyle: { color: '#00D4FF', width: 1.6 },
        areaStyle: { color: 'rgba(0,212,255,0.12)' },
        markLine: {
          silent: true,
          symbol: 'none',
          label: { show: false },
          lineStyle: { color: 'rgba(148,163,184,0.85)', type: 'dashed', width: 1 },
          data: [{ xAxis: cursorSec.value }],
        },
      },
      {
        name: '速度',
        type: 'line',
        yAxisIndex: 1,
        showSymbol: false,
        data: data.map((t) => [timeOf(t), Number(speedOf(t).toFixed(2))]),
        lineStyle: { color: '#f59e0b', width: 1.3 },
      },
    ],
  }
})

watch(mapMode, (mode) => {
  if (mode === '3d' && samples.value.length) {
    nextTick(() => focus3d())
  }
})

watch(
  () => route.query.device,
  (id) => {
    if (typeof id === 'string' && id && id !== deviceId.value) {
      deviceId.value = id
      load()
    }
  }
)

/**
 * 二分 + 线性插值：让机标在样本之间平滑移动，而不是逐帧跳变。
 * 姿态角按最短弧插值，避免 359°→1° 时反转一圈。
 * @param {number} ts
 */
function sampleAt(ts) {
  const list = samples.value
  if (!list.length) return null
  if (ts <= list[0].ts_ms) return list[0]
  const last = list[list.length - 1]
  if (ts >= last.ts_ms) return last
  let lo = 0
  let hi = list.length - 1
  while (lo + 1 < hi) {
    const mid = (lo + hi) >> 1
    if (list[mid].ts_ms <= ts) lo = mid
    else hi = mid
  }
  const a = list[lo]
  const b = list[hi]
  const span = b.ts_ms - a.ts_ms
  const r = span > 0 ? (ts - a.ts_ms) / span : 0
  const mix = (x, y) => x + (y - x) * r
  return {
    ...a,
    ts_ms: ts,
    lat: mix(a.lat, b.lat),
    lon: mix(a.lon, b.lon),
    alt_amsl: mix(a.alt_amsl, b.alt_amsl),
    rel_alt: mix(a.rel_alt, b.rel_alt),
    vx: mix(a.vx, b.vx),
    vy: mix(a.vy, b.vy),
    vz: mix(a.vz, b.vz),
    roll: mix(a.roll, b.roll),
    pitch: mix(a.pitch, b.pitch),
    yaw: a.yaw + shortestAngle(a.yaw, b.yaw) * r,
    battery_pct: mix(a.battery_pct, b.battery_pct),
    voltage: mix(a.voltage, b.voltage),
    link_quality: mix(a.link_quality, b.link_quality),
  }
}

/**
 * @param {number} from
 * @param {number} to
 */
function shortestAngle(from, to) {
  let delta = (to - from) % (Math.PI * 2)
  if (delta > Math.PI) delta -= Math.PI * 2
  if (delta < -Math.PI) delta += Math.PI * 2
  return delta
}

/** @param {any} t */
function speedOf(t) {
  return Math.hypot(Number(t.vx) || 0, Number(t.vy) || 0, Number(t.vz) || 0)
}

function haversine(a, b) {
  const R = 6371000
  const rad = Math.PI / 180
  const dLat = (b.lat - a.lat) * rad
  const dLon = (b.lon - a.lon) * rad
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(a.lat * rad) * Math.cos(b.lat * rad) * Math.sin(dLon / 2) ** 2
  return 2 * R * Math.asin(Math.min(1, Math.sqrt(h)))
}

/** 轨迹包围盒中心与跨度（米），用于 3D 视角。 */
function trackMetrics() {
  const list = track.value
  if (!list.length) return { lon: 121.4737, lat: 31.2304, span: 5000 }
  let minLat = 90
  let maxLat = -90
  let minLon = 180
  let maxLon = -180
  for (const p of list) {
    if (p.lat < minLat) minLat = p.lat
    if (p.lat > maxLat) maxLat = p.lat
    if (p.lon < minLon) minLon = p.lon
    if (p.lon > maxLon) maxLon = p.lon
  }
  const midLat = (minLat + maxLat) / 2
  const latSpan = (maxLat - minLat) * 111320
  const lonSpan = (maxLon - minLon) * 111320 * Math.cos((midLat * Math.PI) / 180)
  return {
    lon: (minLon + maxLon) / 2,
    lat: midLat,
    span: Math.max(latSpan, lonSpan, 200),
  }
}

function focus3d() {
  if (mapMode.value !== '3d') return
  const m = trackMetrics()
  globeRef.value?.flyTo(m.lon, m.lat, Math.max(1200, m.span * 2))
}

function play() {
  if (samples.value.length < 2) return
  if (currentTs.value >= t1.value - 0.5) currentTs.value = t0.value
  playing.value = true
  lastFrame = performance.now()
  cancelAnimationFrame(raf)
  raf = requestAnimationFrame(step)
}

function pause() {
  playing.value = false
  if (raf) {
    cancelAnimationFrame(raf)
    raf = 0
  }
}

function togglePlay() {
  if (playing.value) pause()
  else play()
}

/** @param {number} now */
function step(now) {
  if (!playing.value) return
  // 切后台再回来时 performance.now 会大幅跳变，限制单帧推进量。
  const delta = Math.min(Math.max(0, now - lastFrame), 120)
  lastFrame = now
  currentTs.value = Math.min(t1.value, currentTs.value + delta * speed.value)
  if (currentTs.value >= t1.value) {
    playing.value = false
    raf = 0
    return
  }
  raf = requestAnimationFrame(step)
}

/** @param {number} direction -1 上一帧 / 1 下一帧 */
function jump(direction) {
  const list = samples.value
  if (!list.length) return
  pause()
  const index = nearestIndex(currentTs.value)
  const next = Math.min(list.length - 1, Math.max(0, index + direction))
  currentTs.value = list[next].ts_ms
}

/** @param {number} ts */
function nearestIndex(ts) {
  const list = samples.value
  if (!list.length) return -1
  let lo = 0
  let hi = list.length - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (list[mid].ts_ms < ts) lo = mid + 1
    else hi = mid
  }
  if (lo > 0 && Math.abs(list[lo - 1].ts_ms - ts) <= Math.abs(list[lo].ts_ms - ts)) {
    return lo - 1
  }
  return lo
}

function restart() {
  if (!samples.value.length) return
  pause()
  currentTs.value = t0.value
}

async function load() {
  if (!deviceId.value) {
    ElMessage.warning('请选择无人机')
    return
  }
  if (
    !Array.isArray(range.value) ||
    range.value.length !== 2 ||
    !range.value[0] ||
    !range.value[1]
  ) {
    ElMessage.warning('请选择查询时间段')
    return
  }
  const start = range.value[0].getTime()
  const end = range.value[1].getTime()
  if (end <= start) {
    ElMessage.warning('结束时间必须晚于开始时间')
    return
  }
  const seq = ++loadSeq
  loading.value = true
  pause()
  try {
    const list = await store.fetchReplay(deviceId.value, start, end)
    if (seq !== loadSeq) return
    // GPS 未定位时服务端仍会上报 (0,0)，直接入图会拉出一条跨洲折线。
    samples.value = list.filter((t) => !(t.lat === 0 && t.lon === 0))
    currentTs.value = samples.value[0]?.ts_ms || 0
    if (samples.value.length) {
      nextTick(() => focus3d())
    } else {
      ElMessage.info('该时间窗内没有遥测历史')
    }
  } catch (e) {
    if (seq !== loadSeq) return
    samples.value = []
    ElMessage.error(e?.response?.data?.message || e?.message || '轨迹查询失败')
  } finally {
    if (seq === loadSeq) loading.value = false
  }
}

function onRangeChange() {
  if (
    Array.isArray(range.value) &&
    range.value.length === 2 &&
    range.value[0] &&
    range.value[1]
  ) {
    load()
  }
}

function onDeviceChange(id) {
  if (!id) return
  router.replace({ query: { ...route.query, device: id } }).catch(() => {})
  load()
}

function onKeydown(ev) {
  const tag = ev.target?.tagName
  if (['INPUT', 'TEXTAREA', 'SELECT', 'BUTTON'].includes(tag)) return
  if (ev.code === 'Space') {
    ev.preventDefault()
    togglePlay()
  } else if (ev.code === 'ArrowLeft') {
    ev.preventDefault()
    jump(-1)
  } else if (ev.code === 'ArrowRight') {
    ev.preventDefault()
    jump(1)
  }
}

function pad2(value) {
  return String(value).padStart(2, '0')
}

/** @param {number} ms */
function formatClock(ms) {
  if (!ms) return '--:--:--'
  const d = new Date(ms)
  return `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`
}

/** @param {number} ms */
function formatDateTime(ms) {
  if (!ms) return ''
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`
}

/** @param {number} ms */
function formatDuration(ms) {
  const total = Math.max(0, Math.round((ms || 0) / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  return h > 0 ? `${h}:${pad2(m)}:${pad2(s)}` : `${pad2(m)}:${pad2(s)}`
}

/** @param {number} seconds */
function formatSec(seconds) {
  return formatDuration(Number(seconds) * 1000)
}

/** @param {number} meters */
function formatDistance(meters) {
  if (!meters) return '0 m'
  return meters >= 1000 ? `${(meters / 1000).toFixed(2)} km` : `${meters.toFixed(0)} m`
}

/**
 * @param {number} value
 * @param {number} digits
 */
function formatNum(value, digits) {
  return (Number(value) || 0).toFixed(digits)
}

onMounted(async () => {
  const now = Date.now()
  range.value = [new Date(now - 15 * 60 * 1000), new Date(now)]
  window.addEventListener('keydown', onKeydown)
  await store.fetchDrones()
  const queryId = typeof route.query.device === 'string' ? route.query.device : ''
  deviceId.value = store.drones.some((d) => d.device_id === queryId)
    ? queryId
    : store.drones[0]?.device_id || ''
  await load()
})

onUnmounted(() => {
  pause()
  window.removeEventListener('keydown', onKeydown)
})
</script>

<style scoped>
.replay-header {
  flex-wrap: wrap;
  gap: 10px;
}
.header-left {
  display: flex;
  align-items: baseline;
  gap: 12px;
  min-width: 0;
}
.header-sub {
  font-size: 12px;
  color: var(--text-muted);
  white-space: nowrap;
}
.replay-toolbar {
  flex-wrap: wrap;
}
.replay-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 340px;
  gap: 10px;
  padding: 10px;
}
.stage {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}
.map-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  background: #0f172a;
}
.map-wrap :deep(.drone-map2d),
.map-wrap :deep(.cesium-globe) {
  height: 100%;
}
.map-mode {
  position: absolute;
  top: 10px;
  right: 10px;
  z-index: 5;
  padding: 2px;
  border-radius: 8px;
  background: rgba(15, 23, 42, 0.6);
  backdrop-filter: blur(4px);
}
.map-mode :deep(.el-radio-button__inner) {
  background: transparent;
  color: #e2e8f0;
  border-color: rgba(255, 255, 255, 0.28);
}
.map-mode :deep(.el-radio-button.is-active .el-radio-button__inner) {
  background: var(--color-accent);
  border-color: var(--color-accent);
  color: #04222b;
}
.map-overlay {
  position: absolute;
  inset: 0;
  z-index: 4;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  font-size: 13px;
  color: #cbd5e1;
  background: rgba(8, 15, 30, 0.55);
  backdrop-filter: blur(2px);
  pointer-events: none;
}
.map-overlay .el-icon {
  font-size: 26px;
}
.map-overlay.empty .overlay-hint {
  font-size: 11px;
  color: #94a3b8;
}
.timeline-card {
  flex-shrink: 0;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-surface);
  padding: 10px 14px 6px;
}
.timeline-top {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
}
.transport {
  display: flex;
  gap: 8px;
}
.clock {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-family: var(--font-mono);
}
.clock-current {
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.5px;
}
.clock-sep {
  color: var(--text-muted);
}
.clock-total {
  font-size: 12px;
  color: var(--text-muted);
}
.progress-tag {
  margin-left: 6px;
}
.speed-group {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}
.speed-label {
  font-size: 11px;
  color: var(--text-muted);
}
.timeline-slider {
  margin-top: 2px;
}
.timeline-slider :deep(.el-slider) {
  padding: 0 4px;
}
.timeline-ticks {
  display: flex;
  justify-content: space-between;
  margin-top: -4px;
  font-size: 10px;
  color: var(--text-muted);
  font-family: var(--font-mono);
}
.side-rail {
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-surface);
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
.stat-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px 10px;
}
.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 11px;
  color: var(--text-muted);
}
.stat b {
  font-size: 13px;
  color: var(--text-primary);
  font-family: var(--font-mono);
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
.t-item.wide {
  grid-column: span 2;
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
.mono {
  font-family: var(--font-mono);
}
.profile-chart {
  width: 100%;
  height: 170px;
}
@media (max-width: 1280px) {
  .replay-body {
    grid-template-columns: minmax(0, 1fr) 300px;
  }
}
</style>
