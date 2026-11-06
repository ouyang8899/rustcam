<!-- @ts-check -->
<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">航线规划</h1>
        <span class="header-sub">在地球上点击添加航点，保存后可一键执行</span>
      </div>
      <div class="page-toolbar">
        <el-button :icon="Plus" @click="newRoute">新建航线</el-button>
        <el-button type="primary" :disabled="!canSave" @click="save">保存</el-button>
        <el-select
          v-model="executeTarget"
          size="small"
          placeholder="执行无人机（单架）"
          style="width: 170px"
        >
          <el-option
            v-for="d in executeCandidates"
            :key="d.device_id"
            :label="d.name || d.device_id"
            :value="d.device_id"
          />
        </el-select>
        <el-input-number
          v-model="executeOffset"
          :min="-200"
          :max="200"
          :step="10"
          size="small"
          controls-position="right"
          style="width: 120px"
        />
        <span class="offset-unit">m 横向</span>
        <el-button
          type="success"
          :disabled="!form.id || waypoints.length === 0 || !executeTarget"
          @click="execute"
        >执行</el-button>
      </div>
    </div>

    <div class="planner-body">
      <aside class="route-rail">
        <div class="rail-title">航线列表</div>
        <div
          v-for="r in store.routes"
          :key="r.id"
          class="route-item"
          :class="{ active: r.id === form.id }"
          @click="loadRoute(r)"
        >
          <div class="route-name">{{ r.name }}</div>
          <div class="route-meta">{{ routeScope(r) }} · {{ r.waypoints.length }} 航点</div>
        </div>
        <el-empty v-if="store.routes.length === 0" description="暂无航线" :image-size="60" />

        <div class="rail-title" style="margin-top: 14px">航线属性</div>
        <el-form label-position="top" size="small">
          <el-form-item label="名称">
            <el-input v-model="form.name" placeholder="如：东区巡检 A 线" />
          </el-form-item>
          <el-form-item label="适用无人机（用于分发，可多选）">
            <el-select
              v-model="form.device_ids"
              multiple
              collapse-tags
              clearable
              style="width: 100%"
              placeholder="不选 = 全部无人机"
            >
              <el-option
                v-for="d in store.drones"
                :key="d.device_id"
                :label="d.name || d.device_id"
                :value="d.device_id"
              />
            </el-select>
          </el-form-item>
          <el-form-item label="启用">
            <el-switch v-model="form.enabled" />
          </el-form-item>
          <el-form-item label="任务结束后">
            <el-select v-model="form.end_action" style="width: 100%">
              <el-option label="原地悬停" value="hold" />
              <el-option label="自动返航" value="rtl" />
              <el-option label="原地降落" value="land" />
            </el-select>
          </el-form-item>
        </el-form>

        <div class="rail-title">航点（{{ waypoints.length }}）</div>
        <div v-for="(w, i) in waypoints" :key="i" class="wp-item">
          <div class="wp-head">
            <span>WP{{ i + 1 }}</span>
            <div>
              <el-button text size="small" :disabled="i === 0" @click="move(i, -1)">↑</el-button>
              <el-button text size="small" :disabled="i === waypoints.length - 1" @click="move(i, 1)">↓</el-button>
              <el-button text size="small" type="danger" @click="removeWp(i)">×</el-button>
            </div>
          </div>
          <div class="wp-grid">
            <el-input-number v-model="w.lat" :precision="6" :step="0.0001" controls-position="right" size="small" />
            <el-input-number v-model="w.lon" :precision="6" :step="0.0001" controls-position="right" size="small" />
            <el-input-number v-model="w.alt" :min="1" :max="500" controls-position="right" size="small" />
            <el-input-number v-model="w.speed" :min="1" :max="30" controls-position="right" size="small" />
          </div>
          <div class="wp-foot">
            <el-input-number v-model="w.loiter_sec" :min="0" :max="600" size="small" controls-position="right" />
            <span class="wp-unit">秒悬停</span>
          </div>
        </div>
        <el-empty v-if="waypoints.length === 0" description="点击地图添加航点" :image-size="48" />
      </aside>

      <div class="planner-map">
        <CesiumGlobe
          ref="globeRef"
          :drones="store.drones"
          :selected-id="executeTarget"
          :fences="store.fences"
          :route="waypoints"
          :draft-points="[]"
          draw-mode="waypoint"
          @map-click="addWaypoint"
        >
          <template #hint>
            <el-tag type="warning" effect="dark">点击地球添加航点</el-tag>
          </template>
        </CesiumGlobe>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import CesiumGlobe from '../../components/drone/CesiumGlobe.vue'

const store = useDroneStore()
const globeRef = ref(null)
const waypoints = ref([])

const form = reactive({
  id: '',
  name: '',
  /** 适用无人机列表；空 = 全部（航线模板）。 */
  device_ids: [],
  enabled: true,
  /** 任务结束动作：hold / rtl / land。 */
  end_action: 'hold',
})

/** 执行目标无人机：航线可复用，执行时必须明确一架。 */
const executeTarget = ref('')
/** 本次执行的横向偏移（米，正=右，负=左），用于同航线多机分道。 */
const executeOffset = ref(0)
const executeCandidates = computed(() => {
  if (form.device_ids.length === 0) return store.drones
  return store.drones.filter((d) => form.device_ids.includes(d.device_id))
})

const canSave = computed(() => !!form.name.trim() && waypoints.value.length > 0)

function droneName(deviceId) {
  return store.drones.find((d) => d.device_id === deviceId)?.name || deviceId
}

function routeScope(route) {
  const ids = route.device_ids?.length
    ? route.device_ids
    : route.device_id
      ? [route.device_id]
      : []
  if (ids.length === 0) return '全部无人机'
  return `适用 ${ids.map(droneName).join('、')}`
}

watch(() => form.device_ids, () => pickExecuteTarget(), { deep: true })

function pickExecuteTarget() {
  if (executeCandidates.value.some((d) => d.device_id === executeTarget.value)) return
  executeTarget.value = executeCandidates.value[0]?.device_id || ''
}

function newRoute() {
  form.id = ''
  form.name = ''
  form.device_ids = []
  form.enabled = true
  form.end_action = 'hold'
  waypoints.value = []
  executeTarget.value = store.drones[0]?.device_id || ''
}

function loadRoute(route) {
  form.id = route.id
  form.name = route.name
  form.device_ids = route.device_ids?.length
    ? [...route.device_ids]
    : route.device_id
      ? [route.device_id]
      : []
  form.enabled = route.enabled
  form.end_action = route.end_action || 'hold'
  waypoints.value = route.waypoints.map(w => ({ ...w }))
  const candidates = form.device_ids.length
    ? store.drones.filter((d) => form.device_ids.includes(d.device_id))
    : store.drones
  executeTarget.value = candidates[0]?.device_id || ''
  const first = waypoints.value[0]
  if (first) globeRef.value?.flyTo(first.lon, first.lat, 30000)
}

function addWaypoint({ lat, lon }) {
  const alt = waypoints.value.length > 0 ? waypoints.value[waypoints.value.length - 1].alt : 30
  waypoints.value.push({
    lat: Number(lat.toFixed(6)),
    lon: Number(lon.toFixed(6)),
    alt,
    speed: 8,
    loiter_sec: 0,
    action: 'waypoint',
  })
}

function removeWp(index) {
  waypoints.value.splice(index, 1)
}

function move(index, delta) {
  const target = index + delta
  if (target < 0 || target >= waypoints.value.length) return
  const [item] = waypoints.value.splice(index, 1)
  waypoints.value.splice(target, 0, item)
}

async function save() {
  if (!canSave.value) {
    ElMessage.warning('请填写名称并至少添加一个航点')
    return
  }
  const payload = {
    id: form.id,
    name: form.name,
    device_ids: [...form.device_ids],
    enabled: form.enabled,
    end_action: form.end_action,
    waypoints: waypoints.value.map(w => ({
      lat: Number(w.lat),
      lon: Number(w.lon),
      alt: Number(w.alt),
      speed: Number(w.speed) || 8,
      loiter_sec: Number(w.loiter_sec) || 0,
      action: w.action || 'waypoint',
    })),
  }
  const saved = await store.saveRoute(payload)
  form.id = saved.id
}

async function execute() {
  if (!executeTarget.value) {
    ElMessage.warning('请选择执行航线的无人机')
    return
  }
  try {
    await ElMessageBox.confirm(
      `用「${droneName(executeTarget.value)}」执行航线「${form.name}」？将按顺序飞往 ${waypoints.value.length} 个航点。\n` +
        `横向偏移：${executeOffset.value}m（正=右，负=左）。`,
      '执行航线',
      { type: 'warning', confirmButtonText: '执行', cancelButtonText: '取消' }
    )
  } catch (e) {
    return
  }
  const c = await store.acquireControl(executeTarget.value, 'flight', 10, 300)
  if (!c?.granted) return
  try {
    await store.executeRoute(form.id, executeTarget.value, executeOffset.value)
  } catch (e) {
    ElMessage.error(e?.response?.data?.message || e?.message || '航线执行失败')
  }
}

onMounted(async () => {
  await Promise.all([store.fetchDrones(), store.fetchRoutes(), store.fetchFences()])
  if (store.routes.length > 0) {
    loadRoute(store.routes[0])
  } else {
    newRoute()
  }
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
.offset-unit {
  font-size: 11px;
  color: var(--text-muted);
  white-space: nowrap;
}
.planner-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 10px;
  padding: 10px;
}
.route-rail {
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
.route-item {
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid transparent;
  cursor: pointer;
  margin-bottom: 6px;
}
.route-item:hover {
  background: var(--bg-elevated);
}
.route-item.active {
  border-color: var(--color-accent);
  background: var(--bg-elevated);
}
.route-name {
  font-size: 13px;
  font-weight: 600;
}
.route-meta {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-mono);
  margin-top: 2px;
}
.wp-item {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px;
  margin-bottom: 8px;
  background: var(--bg-base);
}
.wp-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.wp-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}
.wp-grid :deep(.el-input-number) {
  width: 100%;
}
.wp-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
}
.wp-foot :deep(.el-input-number) {
  width: 110px;
}
.wp-unit {
  font-size: 11px;
  color: var(--text-muted);
}
.planner-map {
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.planner-map :deep(.cesium-globe) {
  height: 100%;
}
</style>
