<!-- @ts-check -->
<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">电子围栏</h1>
        <span class="header-sub">禁飞区 / 允许区，越界自动告警并阻止非法定点</span>
      </div>
      <div class="page-toolbar">
        <el-radio-group v-model="mapMode" size="small">
          <el-radio-button value="2d">2D 绘制</el-radio-button>
          <el-radio-button value="3d">3D 绘制</el-radio-button>
        </el-radio-group>
        <el-button :icon="Plus" @click="newFence">新建围栏</el-button>
        <el-button type="primary" :disabled="!canSave" @click="save">保存</el-button>
      </div>
    </div>

    <div class="fence-body">
      <aside class="fence-rail">
        <div class="rail-title">围栏列表</div>
        <div
          v-for="f in store.fences"
          :key="f.id"
          class="fence-item"
          :class="{ active: f.id === form.id }"
          @click="loadFence(f)"
        >
          <div class="fence-name">
            <span class="dot" :style="{ background: fenceColors(f.type).fill }"></span>
            {{ f.name }}
          </div>
          <div class="fence-meta">
            {{ fenceRuleLabel(f.type) }} ·
            {{ f.shape === 'circle' ? `半径 ${Math.round(f.radius_m)}m` : '多边形' }}
            · {{ f.min_alt }}-{{ f.max_alt }}m ·
            {{ fenceScope(f) }}
          </div>
        </div>
        <el-empty v-if="store.fences.length === 0" description="暂无围栏" :image-size="60" />

        <div class="rail-title" style="margin-top: 14px">围栏属性</div>
        <el-form label-position="top" size="small">
          <el-form-item label="名称">
            <el-input v-model="form.name" placeholder="如：机场禁飞区" />
          </el-form-item>
          <el-form-item label="规则">
            <el-radio-group v-model="form.type">
              <el-radio-button value="no_fly">禁飞区</el-radio-button>
              <el-radio-button value="allowed">允许区</el-radio-button>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="形状">
            <el-radio-group v-model="form.shape" @change="onShapeChange">
              <el-radio-button value="polygon">多边形</el-radio-button>
              <el-radio-button value="circle">圆形</el-radio-button>
            </el-radio-group>
          </el-form-item>
          <el-form-item label="适用无人机（可多选）">
            <el-select
              v-model="form.drone_tags"
              multiple
              collapse-tags
              clearable
              style="width: 100%"
              placeholder="不选 = 全部无人机"
            >
              <el-option
                v-for="d in store.drones"
                :key="d.drone_tag"
                :label="d.name || d.drone_tag"
                :value="d.drone_tag"
              />
            </el-select>
          </el-form-item>
          <el-form-item label="高度范围（米）">
            <div class="alt-row">
              <el-input-number v-model="form.min_alt" :min="0" :max="1000" controls-position="right" />
              <span>—</span>
              <el-input-number v-model="form.max_alt" :min="0" :max="1000" controls-position="right" />
            </div>
          </el-form-item>
          <el-form-item label="启用">
            <el-switch v-model="form.enabled" />
          </el-form-item>
        </el-form>

        <div v-if="mapMode === '2d'" class="draw-hint">
          <template v-if="form.shape === 'polygon'">
            点击地图添加顶点，右键结束（至少 3 个），再点顶部“保存”。
          </template>
          <template v-else>
            点击设置圆心，移动鼠标预览半径，右键结束，再点顶部“保存”。
          </template>
          <el-button text size="small" @click="clearDraft">清空</el-button>
        </div>
        <div v-else class="draw-hint">
          <template v-if="form.shape === 'polygon'">
            <template v-if="drawingActive">
              已选 {{ points.length }} 个顶点，点击地图继续添加，右键结束（至少 3 个）。
            </template>
            <template v-else>点击地图开始新绘制（将替换当前图形）。</template>
          </template>
          <template v-else>
            <template v-if="!drawingActive">点击地图设置圆心（将替换当前圆形）。</template>
            <template v-else-if="circleStep === 1">移动鼠标预览半径，右键结束。</template>
            <template v-else>可点击调整半径，右键结束。</template>
          </template>
          <el-button text size="small" @click="clearDraft">重选</el-button>
        </div>
      </aside>

      <div class="fence-map">
        <GeofenceDraw
          v-if="mapMode === '2d'"
          ref="drawRef"
          :center="mapCenter"
          :shape="form.shape"
          :rule="form.type"
          :fences="otherFences"
          :initial-geometry="initialGeometry"
          :initial-circle="initialCircle"
          @change="onDrawChange"
        />
        <CesiumGlobe
          v-else
          ref="globeRef"
          :drones="store.drones"
          :fences="otherFences"
          :draw-mode="form.shape"
          :draft-rule="form.type"
          :drawing-active="drawingActive"
          :draft-points="points"
          :draft-circle="draftCircle"
          @map-click="onMapClick"
          @finish="onDrawFinish"
        >
          <template #hint>
            <el-tag type="warning" effect="dark">
              {{
                form.shape === 'polygon'
                  ? drawingActive
                    ? '点击继续添加顶点，右键结束'
                    : '点击地球开始新绘制'
                  : drawingActive
                    ? circleStep === 1
                      ? '移动鼠标预览半径，右键结束'
                      : '点击调整半径，右键结束'
                    : '点击地球设置圆心'
              }}
            </el-tag>
          </template>
        </CesiumGlobe>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import { fenceColors, fenceRuleLabel } from '../../utils/fenceStyle'
import CesiumGlobe from '../../components/drone/CesiumGlobe.vue'
import GeofenceDraw from '../../components/drone/GeofenceDraw.vue'

const store = useDroneStore()
const globeRef = ref(null)
const drawRef = ref(null)
const points = ref([])
const circle = reactive({ lat: 0, lon: 0, radius_m: 0 })
const circleStep = ref(0)
/** 规则 §10.5：多边形与圆形默认使用 terra-draw 二维绘制，三维模式走 Cesium。 */
const mapMode = ref('2d')
/** 三维正在绘制：首次点击开启新图形，右键结束后回到预览态。 */
const drawingActive = ref(false)

const form = reactive({
  id: '',
  name: '',
  type: 'no_fly',
  shape: 'polygon',
  drone_tags: [],
  min_alt: 0,
  max_alt: 120,
  enabled: true,
})

const draftCircle = computed(() =>
  form.shape === 'circle' && circleStep.value > 0
    ? { lat: circle.lat, lon: circle.lon, radius_m: circle.radius_m }
    : null
)

const otherFences = computed(() => store.fences.filter((f) => f.id !== form.id))

const initialGeometry = computed(() => {
  if (form.shape !== 'polygon' || points.value.length < 3) return null
  const ring = points.value.map((p) => [p.lon, p.lat])
  ring.push([...ring[0]])
  return { type: 'Polygon', coordinates: [ring] }
})

const initialCircle = computed(() =>
  form.shape === 'circle' && circle.radius_m > 0
    ? { lon: circle.lon, lat: circle.lat, radius_m: circle.radius_m }
    : null
)

const mapCenter = ref({ lat: 31.2304, lon: 121.4737, zoom: 10 })

const canSave = computed(() => {
  if (!form.name.trim()) return false
  if (form.shape === 'circle') return circle.radius_m > 0
  return points.value.length >= 3
})

function clearDraw() {
  try {
    drawRef.value?.clear(true)
  } catch (e) {
    console.error('[fence] 清空二维画布失败', e)
  }
}

function clearDraft() {
  drawingActive.value = false
  clearDraw()
  if (form.shape === 'circle') resetCircle()
  else points.value = []
}

/** 无人机显示名；空 drone_tag 表示围栏对全部无人机生效。 */
function droneName(deviceId) {
  if (!deviceId) return '全部无人机'
  return store.drones.find((d) => d.drone_tag === deviceId)?.name || deviceId
}

/** 围栏适用范围文案：全部 / 多架名称。 */
function fenceScope(fence) {
  const ids = fence.drone_tags?.length
    ? fence.drone_tags
    : fence.drone_tag
      ? [fence.drone_tag]
      : []
  if (ids.length === 0) return '全部无人机'
  return `适用 ${ids.map(droneName).join('、')}`
}

function newFence() {
  clearDraw()
  form.id = ''
  form.name = ''
  form.type = 'no_fly'
  form.shape = 'polygon'
  form.drone_tags = []
  form.min_alt = 0
  form.max_alt = 120
  form.enabled = true
  points.value = []
  resetCircle()
  drawingActive.value = false
}

function resetCircle() {
  circle.lat = 0
  circle.lon = 0
  circle.radius_m = 0
  circleStep.value = 0
}

function onShapeChange() {
  clearDraw()
  points.value = []
  resetCircle()
  drawingActive.value = false
}

function loadFence(fence) {
  form.id = fence.id
  form.name = fence.name
  form.type = fence.type || 'no_fly'
  form.shape = fence.shape || 'polygon'
  form.drone_tags = fence.drone_tags?.length
    ? [...fence.drone_tags]
    : fence.drone_tag
      ? [fence.drone_tag]
      : []
  form.min_alt = fence.min_alt || 0
  form.max_alt = fence.max_alt || 120
  form.enabled = fence.enabled
  clearDraw()
  points.value = []
  resetCircle()
  drawingActive.value = false
  if (form.shape === 'circle') {
    circle.lat = fence.center_lat
    circle.lon = fence.center_lon
    circle.radius_m = fence.radius_m
    circleStep.value = 1
  } else {
    try {
      const geo = JSON.parse(fence.geojson)
      const ring = geo.type === 'Polygon' ? geo.coordinates[0] : geo.coordinates?.[0]?.[0] || []
      points.value = ring.slice(0, -1).map(p => ({ lon: p[0], lat: p[1] }))
    } catch (e) {
      points.value = []
    }
  }
  const c = points.value[0] || (form.shape === 'circle' ? { lon: circle.lon, lat: circle.lat } : null)
  if (c) {
    mapCenter.value = { lat: c.lat, lon: c.lon, zoom: 12 }
    globeRef.value?.flyTo(c.lon, c.lat, 30000)
  }
}

function haversine(a, b) {
  const R = 6371000
  const dLat = ((b.lat - a.lat) * Math.PI) / 180
  const dLon = ((b.lon - a.lon) * Math.PI) / 180
  const lat1 = (a.lat * Math.PI) / 180
  const lat2 = (b.lat * Math.PI) / 180
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2
  return 2 * R * Math.asin(Math.sqrt(h))
}

function onMapClick({ lon, lat }) {
  if (form.shape === 'polygon') {
    // 与 2D 一致：未在绘制时首次点击清除旧图形，重新开始。
    if (!drawingActive.value) {
      points.value = []
      drawingActive.value = true
    }
    points.value.push({ lon: Number(lon.toFixed(6)), lat: Number(lat.toFixed(6)) })
    return
  }
  if (!drawingActive.value) {
    resetCircle()
    circle.lat = Number(lat.toFixed(6))
    circle.lon = Number(lon.toFixed(6))
    circleStep.value = 1
    drawingActive.value = true
    return
  }
  circle.radius_m = Math.round(haversine({ lat: circle.lat, lon: circle.lon }, { lat, lon }))
  if (circle.radius_m < 10) circle.radius_m = 10
  circleStep.value = 2
}

/**
 * 三维右键结束：多边形停止加点，圆形用右键位置落定半径。
 * @param {{ lon: number, lat: number } | null} pos
 */
function onDrawFinish(pos) {
  if (!drawingActive.value) return
  if (form.shape === 'polygon') {
    if (points.value.length < 3) {
      ElMessage.warning('请至少绘制 3 个顶点')
      return
    }
    drawingActive.value = false
    return
  }
  if (circleStep.value === 1) {
    if (!pos) return
    circle.radius_m = Math.round(haversine({ lat: circle.lat, lon: circle.lon }, pos))
    if (circle.radius_m < 10) circle.radius_m = 10
    circleStep.value = 2
  }
  drawingActive.value = false
}

/** 圆环各顶点均值即圆心；半径取各顶点到圆心的平均距离。 */
function circleFromRing(ring) {
  const pts = ring.slice(0, -1)
  const center = pts.reduce(
    (acc, p) => ({ lon: acc.lon + p[0], lat: acc.lat + p[1] }),
    { lon: 0, lat: 0 }
  )
  center.lon /= pts.length
  center.lat /= pts.length
  const radius =
    pts.reduce((sum, p) => sum + haversine(center, { lon: p[0], lat: p[1] }), 0) / pts.length
  return { lon: center.lon, lat: center.lat, radius_m: radius }
}

/**
 * terra-draw 变化回调：把绘制结果实时回填为顶点/圆心，顶栏“保存”直接复用。
 * @param {any} geometry
 */
function onDrawChange(geometry) {
  const ring = geometry?.coordinates?.[0] || []
  if (form.shape === 'circle') {
    if (ring.length < 4) {
      resetCircle()
      return
    }
    const c = circleFromRing(ring)
    const lon = Number(c.lon.toFixed(6))
    const lat = Number(c.lat.toFixed(6))
    const radius = Math.max(10, Math.round(c.radius_m))
    if (lon === circle.lon && lat === circle.lat && radius === circle.radius_m) return
    circle.lon = lon
    circle.lat = lat
    circle.radius_m = radius
    circleStep.value = 2
    return
  }
  const next = ring.slice(0, -1).map((p) => ({
    lon: Number(p[0].toFixed(6)),
    lat: Number(p[1].toFixed(6)),
  }))
  if (
    next.length === points.value.length &&
    next.every((p, i) => p.lon === points.value[i].lon && p.lat === points.value[i].lat)
  ) {
    return
  }
  points.value = next
}

async function save() {
  if (!canSave.value) {
    ElMessage.warning(form.shape === 'circle' ? '请完成圆形围栏绘制' : '至少需要 3 个顶点')
    return
  }
  let geojson = ''
  if (form.shape === 'polygon') {
    const ring = points.value.map(p => [p.lon, p.lat])
    ring.push([...ring[0]])
    geojson = JSON.stringify({ type: 'Polygon', coordinates: [ring] })
  }
  const payload = {
    id: form.id,
    name: form.name,
    type: form.type,
    shape: form.shape,
    geojson,
    center_lat: form.shape === 'circle' ? circle.lat : 0,
    center_lon: form.shape === 'circle' ? circle.lon : 0,
    radius_m: form.shape === 'circle' ? circle.radius_m : 0,
    min_alt: Number(form.min_alt) || 0,
    max_alt: Number(form.max_alt) || 0,
    drone_tag: form.drone_tags.length === 1 ? form.drone_tags[0] : '',
    drone_tags: [...form.drone_tags],
    enabled: form.enabled,
  }
  const saved = await store.saveFence(payload)
  form.id = saved.id
}

onMounted(async () => {
  await Promise.all([store.fetchDrones(), store.fetchFences()])
  if (store.fences.length > 0) loadFence(store.fences[0])
})
</script>

<style scoped>
.header-left {
  display: flex;
  align-items: baseline;
  gap: 12px;
}
.header-sub {
  font-size: 12px;
  color: var(--text-muted);
}
.fence-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 10px;
  padding: 10px;
}
.fence-rail {
  min-height: 0;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-surface);
  padding: 12px;
}
.rail-title {
  font-size: 12px;
  color: var(--text-muted);
  margin: 4px 0 8px;
}
.fence-item {
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid transparent;
  cursor: pointer;
  margin-bottom: 6px;
}
.fence-item:hover { background: var(--bg-elevated); }
.fence-item.active {
  border-color: var(--color-accent);
  background: var(--bg-elevated);
}
.fence-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
}
.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.dot.red { background: #ef4444; }
.dot.green { background: #22c55e; }
.fence-meta {
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 2px;
}
.alt-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.alt-row :deep(.el-input-number) { width: 110px; }
.draw-hint {
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.6;
}
.fence-map {
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.fence-map :deep(.cesium-globe) { height: 100%; }
</style>
