<!-- @ts-check -->
<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <h1 class="page-title">接入适配器</h1>
        <span class="header-sub">机载链路实体 · 决定谁来接管飞控、监听哪个端口段</span>
      </div>
      <div class="page-toolbar">
        <el-switch
          v-model="store.includeDisabled"
          active-text="显示已停用"
          @change="onToggleDisabled"
        />
        <el-button :icon="Refresh" @click="refresh">刷新</el-button>
        <el-button type="primary" :icon="Plus" @click="openEditor()">新增 Adapter</el-button>
      </div>
    </div>

    <div class="page-body adapter-page">
      <div class="stat-grid">
        <div class="stat-card">
          <div class="stat-label">Adapter 总数</div>
          <div class="stat-value">{{ store.adapters.length }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">在线</div>
          <div class="stat-value online">{{ store.onlineCount }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">启用中</div>
          <div class="stat-value">{{ store.enabledAdapters.length }}</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">承载无人机</div>
          <div class="stat-value">{{ boundTotal }}</div>
        </div>
      </div>

      <el-alert
        type="info"
        :closable="false"
        show-icon
        class="hint"
        title="标识要与适配器进程配置一致"
        description="adapter_id 是适配器接入平台时使用的标识，创建后不可修改；端口段内的端口按飞控系统号区分，一台 adapter 可服务多架无人机。"
      />

      <el-empty v-if="!store.loading && store.adapters.length === 0" description="还没有 adapter 配置">
        <el-button type="primary" @click="openEditor()">创建第一个 Adapter</el-button>
      </el-empty>

      <el-table v-else v-loading="store.loading" :data="store.adapters" class="adapter-table" stripe>
        <el-table-column label="Adapter" min-width="200">
          <template #default="{ row }">
            <div class="adapter-name-cell">
              <span class="status-dot" :class="row.online ? 'online' : 'offline'"></span>
              <div>
                <div class="adapter-name">{{ row.name || row.adapter_id }}</div>
                <div class="adapter-id">{{ row.adapter_id }}</div>
              </div>
            </div>
          </template>
        </el-table-column>
        <el-table-column prop="protocol" label="协议" width="110">
          <template #default="{ row }">
            <el-tag size="small" effect="plain">{{ row.protocol || '—' }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="监听 / 端口段" width="220">
          <template #default="{ row }">
            <span class="mono">{{ row.bind_addr || '0.0.0.0' }}</span>
            <div class="muted mono">{{ row.port_start }}-{{ row.port_end }}</div>
          </template>
        </el-table-column>
        <el-table-column label="飞控" width="120">
          <template #default="{ row }">
            <el-tag size="small" effect="plain">{{ (row.autopilot || 'auto').toUpperCase() }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="在线" width="90">
          <template #default="{ row }">
            <el-tag v-if="row.online" type="success" size="small">已接入</el-tag>
            <el-tag v-else type="info" size="small">未接入</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="90">
          <template #default="{ row }">
            <el-tag v-if="row.enabled" type="success" size="small">启用</el-tag>
            <el-tag v-else type="warning" size="small">已停用</el-tag>
          </template>
        </el-table-column>
        <el-table-column label="承载无人机" width="110">
          <template #default="{ row }">
            <span class="muted">{{ row.bound_drones || 0 }} 架</span>
          </template>
        </el-table-column>
        <el-table-column label="操作" width="190" fixed="right">
          <template #default="{ row }">
            <el-button text type="primary" @click="openEditor(row)">编辑</el-button>
            <el-button v-if="row.enabled" text type="warning" @click="disable(row)">停用</el-button>
            <el-button v-else text type="success" @click="restore(row)">恢复</el-button>
          </template>
        </el-table-column>
      </el-table>
    </div>

    <el-dialog
      v-model="editorVisible"
      :title="editing ? '编辑 Adapter' : '新增 Adapter'"
      width="560px"
      destroy-on-close
    >
      <el-form :model="form" label-width="96px">
        <el-form-item label="标识">
          <el-input
            v-model="form.adapter_id"
            :disabled="editing"
            placeholder="与适配器进程配置一致，如 adapter-mavlink-1"
          />
        </el-form-item>
        <el-form-item label="名称">
          <el-input v-model="form.name" placeholder="如：东场链路" />
        </el-form-item>
        <el-form-item label="协议">
          <el-select v-model="form.protocol" filterable allow-create style="width: 100%">
            <el-option label="MAVLink" value="mavlink" />
          </el-select>
        </el-form-item>
        <el-form-item label="监听地址">
          <el-input v-model="form.bind_addr" placeholder="0.0.0.0" />
        </el-form-item>
        <el-form-item label="端口段">
          <div class="port-row">
            <el-input-number v-model="form.port_start" :min="1" :max="65535" controls-position="right" />
            <span>—</span>
            <el-input-number v-model="form.port_end" :min="1" :max="65535" controls-position="right" />
          </div>
        </el-form-item>
        <el-form-item label="飞控类型">
          <el-select v-model="form.autopilot" style="width: 100%">
            <el-option label="自动识别" value="auto" />
            <el-option label="PX4" value="px4" />
            <el-option label="ArduPilot" value="ardupilot" />
          </el-select>
        </el-form-item>
        <el-form-item label="签名密钥">
          <el-input
            v-model="form.signing_secret"
            type="password"
            show-password
            placeholder="十六进制密钥，留空表示该链路不需要签名"
          />
        </el-form-item>
        <el-form-item label="放行未签名帧">
          <el-switch v-model="form.signing_allow_unsigned" />
        </el-form-item>
        <el-form-item label="重连退避">
          <div class="port-row">
            <el-input-number v-model="form.reconnect_min_secs" :min="1" :max="3600" controls-position="right" />
            <span>—</span>
            <el-input-number v-model="form.reconnect_max_secs" :min="1" :max="3600" controls-position="right" />
          </div>
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="editorVisible = false">取消</el-button>
        <el-button type="primary" @click="save">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { computed, onMounted, reactive, ref } from 'vue'
import { ElMessageBox, ElMessage } from 'element-plus'
import { Plus, Refresh } from '@element-plus/icons-vue'
import { useAdapterStore } from '../../stores/adapterStore'

const store = useAdapterStore()
const editorVisible = ref(false)
const editing = ref(false)

const emptyForm = () => ({
  adapter_id: '',
  name: '',
  protocol: 'mavlink',
  bind_addr: '0.0.0.0',
  port_start: 14540,
  port_end: 14560,
  autopilot: 'auto',
  signing_secret: '',
  signing_allow_unsigned: true,
  reconnect_min_secs: 1,
  reconnect_max_secs: 30,
})

const form = reactive(emptyForm())

const boundTotal = computed(() =>
  store.adapters.reduce((sum, a) => sum + (a.bound_drones || 0), 0)
)

async function refresh() {
  await store.fetchAdapters(store.includeDisabled)
}

/** 切换「显示已停用」：按开关当前值重新拉列表。 */
async function onToggleDisabled(value) {
  await store.fetchAdapters(value)
}

function openEditor(row = null) {
  editing.value = !!row
  Object.assign(form, emptyForm(), row ? { ...row } : {})
  editorVisible.value = true
}

async function save() {
  const id = form.adapter_id.trim()
  if (!id) {
    ElMessage.warning('Adapter 标识不能为空')
    return
  }
  if (form.port_start > form.port_end) {
    ElMessage.warning('端口段的起始端口不能大于结束端口')
    return
  }
  if (form.reconnect_min_secs > form.reconnect_max_secs) {
    ElMessage.warning('重连退避的下限不能大于上限')
    return
  }
  const secret = form.signing_secret.trim()
  if (secret && (!/^[0-9a-fA-F]+$/.test(secret) || secret.length % 2 !== 0)) {
    ElMessage.warning('签名密钥必须是偶数长度的十六进制字符串')
    return
  }
  try {
    await store.saveAdapter({ ...form, adapter_id: id })
    editorVisible.value = false
  } catch (e) {
    // request 拦截器已提示
  }
}

async function disable(row) {
  await ElMessageBox.confirm(
    `停用 Adapter「${row.name || row.adapter_id}」？已绑定的无人机继续运行，但不能再绑定新机。`,
    '停用确认',
    { type: 'warning', confirmButtonText: '停用', cancelButtonText: '取消' }
  )
  await store.disableAdapter(row.adapter_id)
}

async function restore(row) {
  await store.restoreAdapter(row.adapter_id)
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
.adapter-page {
  gap: 16px;
}
.hint {
  margin-bottom: 4px;
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
.adapter-table {
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.adapter-name-cell {
  display: flex;
  align-items: center;
  gap: 10px;
}
.adapter-name {
  font-weight: 600;
}
.adapter-id {
  font-size: 11px;
  color: var(--text-muted);
  font-family: var(--font-mono);
}
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.status-dot.online { background: #22c55e; }
.status-dot.offline { background: var(--text-muted); }
.mono {
  font-family: var(--font-mono);
}
.muted {
  color: var(--text-muted);
  font-size: 12px;
}
.port-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.port-row .el-input-number {
  flex: 1;
}
@media (max-width: 1100px) {
  .stat-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
