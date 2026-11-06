<!-- @ts-check -->
<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">无人机</h1>
        <span class="header-sub">移动眼 · 飞控接入与任务调度</span>
      </div>
      <div class="page-toolbar">
        <el-button :icon="Refresh" @click="refresh">刷新</el-button>
        <el-button type="primary" :icon="Plus" @click="openEditor()">新增无人机</el-button>
      </div>
    </div>

    <div class="page-body drone-page">
      <div class="stat-grid">
        <div class="stat-card">
          <div class="stat-label">无人机总数</div>
          <div class="stat-value">{{ store.drones.length }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">在线</div>
          <div class="stat-value online">{{ store.onlineCount }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">已解锁</div>
          <div class="stat-value warn">{{ armedCount }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">围栏 / 航线</div>
          <div class="stat-value">{{ store.fences.length }} / {{ store.routes.length }}</div>
        </div>
      </div>

      <el-empty v-if="!store.loading && store.drones.length === 0" description="还没有无人机档案">
        <el-button type="primary" @click="openEditor()">创建第一架无人机</el-button>
      </el-empty>

      <el-table v-else v-loading="store.loading" :data="store.drones" class="drone-table" stripe>
        <el-table-column label="无人机" min-width="200">
          <template #default="{ row }">
            <div class="drone-name-cell">
              <span class="status-dot" :class="row.online ? 'online' : 'offline'"></span>
              <div>
                <div class="drone-name">{{ row.name || row.device_id }}</div>
                <div class="drone-id">{{ row.drone_tag || row.device_id }}</div>
              </div>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="model" label="机型" width="130">
          <template #default="{ row }">{{ row.model || '—' }}</template>
        </el-table-column>
        <el-table-column label="飞控 / 模式" width="180">
          <template #default="{ row }">
            <div class="fc-cell">
              <el-tag size="small" effect="plain">{{ (row.autopilot || 'unknown').toUpperCase() }}</el-tag>
              <span class="mode-text">{{ row.mode || '—' }}</span>
            </div>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="110">
          <template #default="{ row }">
            <el-tag v-if="!row.online" type="info" size="small">离线</el-tag>
            <el-tag v-else-if="row.armed" type="warning" size="small">已解锁</el-tag>
            <el-tag v-else type="success" size="small">待命</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="电池 / 链路" width="170">
          <template #default="{ row }">
            <template v-if="row.last_telemetry">
              <el-progress
                :percentage="Math.round(row.last_telemetry.battery_pct || 0)"
                :status="batteryStatus(row.last_telemetry.battery_pct)"
                :stroke-width="6"
                :show-text="false"
              />
              <div class="battery-line">
                <span>{{ Math.round(row.last_telemetry.battery_pct || 0) }}%</span>
                <span>{{ row.last_telemetry.link_quality || 0 }}% 链路</span>
              </div>
            </template>
            <span v-else class="muted">暂无遥测</span>
          </template>
        </el-table-column>
        <el-table-column label="摄像头" width="200">
          <template #default="{ row }">
            <span v-if="row.camera_device_tags?.length" class="muted">
              {{ row.camera_device_tags.length }} 个：{{ row.camera_device_tags.join('、') }}
            </span>
            <span v-else class="muted">未关联</span>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="240" fixed="right">
          <template #default="{ row }">
            <el-button text type="primary" @click="view(row)">查看</el-button>
            <el-button text @click="openEditor(row)">编辑</el-button>
            <el-button text type="danger" @click="remove(row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <el-dialog v-model="editorVisible" :title="editing ? '编辑无人机' : '新增无人机'" width="560px" destroy-on-close>
      <el-form :model="form" label-width="96px">
        <el-form-item label="无人机标识">
          <el-input :model-value="form.drone_tag || '保存后自动生成'" disabled />
        </el-form-item>
        <el-form-item label="设备 ID" required>
          <el-input v-model="form.device_id" :disabled="editing" placeholder="与 adapter 配置的 device_id 一致，如 drone-001" />
        </el-form-item>
        <el-form-item label="名称">
          <el-input v-model="form.name" placeholder="如：东区巡检机" />
        </el-form-item>
        <el-form-item label="机型">
          <el-input v-model="form.model" placeholder="如：M350 / S500" />
        </el-form-item>
        <el-form-item label="飞控类型">
          <el-select v-model="form.autopilot" style="width: 100%">
            <el-option label="自动识别" value="auto" />
            <el-option label="PX4" value="px4" />
            <el-option label="ArduPilot" value="ardupilot" />
          </el-select>
        </el-form-item>
        <el-form-item label="关联摄像头">
          <div class="camera-picker">
            <el-tag
              v-for="tag in form.camera_device_tags"
              :key="tag"
              closable
              size="small"
              @close="removeCameraTag(tag)"
            >{{ tag }}</el-tag>
            <el-button size="small" @click="openCameraDialog">关联设备</el-button>
          </div>
        </el-form-item>
        <el-divider content-position="left">机载链路身份</el-divider>
        <el-form-item label="归属 Adapter">
          <el-input v-model="form.adapter_id" placeholder="由哪台 adapter 接管，如 adapter-mavlink-1" />
        </el-form-item>
        <el-form-item label="唯一标识">
          <el-input v-model="form.unique_id" placeholder="飞控 AUTOPILOT_VERSION.uid，小写十六进制" />
        </el-form-item>
        <el-form-item label="MAVLink 端点">
          <el-input v-model="form.mavlink_addr" placeholder="udpin:10.88.0.2:14540（隧道内地址）" />
        </el-form-item>
        <el-form-item label="协议版本">
          <el-select v-model="form.mavlink_version" style="width: 100%">
            <el-option label="MAVLink 2（可签名）" :value="2" />
            <el-option label="MAVLink 1（不能签名）" :value="1" />
          </el-select>
        </el-form-item>
        <el-form-item label="System ID">
          <el-input-number v-model="form.mavlink_sysid" :min="1" :max="255" style="width: 100%" />
        </el-form-item>
        <el-form-item label="Component ID">
          <el-input-number v-model="form.mavlink_compid" :min="1" :max="255" style="width: 100%" />
        </el-form-item>
        <el-form-item label="签名密钥">
          <el-input
            v-model="form.signing_secret"
            type="password"
            show-password
            placeholder="MAVLink 2 签名密钥（32 字节 hex），留空表示该机不需要签名"
          />
        </el-form-item>
        <el-form-item label="放行未签名帧">
          <el-switch v-model="form.signing_allow_unsigned" />
        </el-form-item>
        <el-form-item label="备注">
          <el-input v-model="form.notes" type="textarea" :rows="2" />
        </el-form-item>
        <el-form-item label="启用">
          <el-switch v-model="form.enabled" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="editorVisible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>

    <DroneCameraDialog
      v-model="cameraDialogVisible"
      :selected-tags="form.camera_device_tags"
      @update:selected-tags="onCameraTagsSelected"
    />
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessageBox, ElMessage } from 'element-plus'
import { Plus, Refresh } from '@element-plus/icons-vue'
import { useDroneStore } from '../../stores/droneStore'
import DroneCameraDialog from '../../components/drone/DroneCameraDialog.vue'

const store = useDroneStore()
const router = useRouter()
const editorVisible = ref(false)
const editing = ref(false)
const cameraDialogVisible = ref(false)
/** 打开编辑器时的关联列表；保存时按差异增删关联。 */
const originalCameraTags = ref([])

const emptyForm = () => ({
  device_id: '',
  name: '',
  model: '',
  autopilot: 'auto',
  drone_tag: '',
  device_tag: '',
  channel_tag: '',
  video_stream: '',
  camera_device_tags: [],
  notes: '',
  enabled: true,
  adapter_id: '',
  unique_id: '',
  mavlink_addr: '',
  mavlink_version: 2,
  mavlink_sysid: 1,
  mavlink_compid: 1,
  signing_secret: '',
  signing_allow_unsigned: true,
})

const form = reactive(emptyForm())

const armedCount = computed(() =>
  store.drones.filter(d => d.last_telemetry?.armed).length
)

function batteryStatus(pct) {
  if (pct <= 20) return 'exception'
  if (pct <= 40) return 'warning'
  return 'success'
}

async function refresh() {
  await Promise.all([store.fetchDrones(), store.fetchRoutes(), store.fetchFences()])
}

function openEditor(row = null) {
  editing.value = !!row
  Object.assign(form, emptyForm(), row ? { ...row } : {})
  form.camera_device_tags = [...(row?.camera_device_tags || [])]
  originalCameraTags.value = [...form.camera_device_tags]
  editorVisible.value = true
}

function openCameraDialog() {
  cameraDialogVisible.value = true
}

function onCameraTagsSelected(tags) {
  form.camera_device_tags = [...tags]
}

function removeCameraTag(tag) {
  form.camera_device_tags = form.camera_device_tags.filter(t => t !== tag)
}

async function save() {
  if (!form.device_id.trim()) {
    ElMessage.warning('请填写设备 ID')
    return
  }
  if (form.unique_id.trim() && !/^[0-9a-f]+$/.test(form.unique_id.trim())) {
    ElMessage.warning('唯一标识必须是十六进制字符串')
    return
  }
  if (form.mavlink_version === 1 && form.signing_secret.trim()) {
    ElMessage.warning('MAVLink 1 没有签名字段，不能配置签名密钥')
    return
  }
  try {
    const saved = await store.saveDrone({ ...form })
    await syncCameras(saved)
    editorVisible.value = false
  } catch (e) {
    // request 拦截器已提示
  }
}

/** 按差异同步摄像头关联：新增调用关联接口，移除调用取消关联接口。 */
async function syncCameras(saved) {
  const droneTag = saved?.drone_tag || ''
  if (!droneTag) {
    if (form.camera_device_tags.length > 0) {
      ElMessage.warning('后端未返回 drone_tag，摄像头关联未保存')
    }
    return
  }
  const original = originalCameraTags.value
  const next = form.camera_device_tags
  for (const tag of next.filter(t => !original.includes(t))) {
    await store.addDroneCamera(droneTag, tag)
  }
  for (const tag of original.filter(t => !next.includes(t))) {
    await store.removeDroneCamera(droneTag, tag)
  }
  await store.fetchDrones()
}

function view(row) {
  router.push({ path: '/drone/live', query: { device: row.device_id } })
}

async function remove(row) {
  await ElMessageBox.confirm(
    `删除无人机档案「${row.name || row.device_id}」？其航线也会一并删除。`,
    '删除确认',
    { type: 'warning', confirmButtonText: '删除', cancelButtonText: '取消' }
  )
  await store.removeDrone(row.device_id)
}

onMounted(refresh)
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
.drone-page {
  gap: 16px;
}
.camera-picker {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}
.stat-card {
  padding: 14px 16px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg-surface);
}
.stat-label {
  font-size: 12px;
  color: var(--text-muted);
}
.stat-value {
  margin-top: 6px;
  font-size: 26px;
  font-weight: 700;
  font-family: var(--font-mono);
}
.stat-value.online { color: #22c55e; }
.stat-value.warn { color: #f97316; }
.drone-table {
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.drone-name-cell {
  display: flex;
  align-items: center;
  gap: 10px;
}
.drone-name {
  font-weight: 600;
}
.drone-id {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-mono);
}
.fc-cell {
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-items: flex-start;
}
.mode-text {
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-secondary);
}
.battery-line {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-muted);
  margin-top: 3px;
}
.muted {
  color: var(--text-muted);
  font-size: 12px;
}
@media (max-width: 1100px) {
  .stat-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
