<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">录制策略</h1>
      <div class="page-toolbar">
        <el-button :icon="Refresh" :loading="loading" @click="fetchPolicies">刷新</el-button>
        <el-button type="primary" :icon="Plus" @click="openCreateDialog">新建策略</el-button>
      </div>
    </div>
    <div class="page-body">
      <DataCard>
        <el-skeleton animated :loading="loading" :rows="5">
          <el-table v-if="policies.length > 0" :data="policies" style="margin-top: 0">
            <el-table-column label="状态" width="80">
              <template #default="{ row }">
                <el-tag :type="row.enabled ? 'success' : 'info'" size="small">{{ row.enabled ? '启用' : '禁用' }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="设备" prop="device_tag" width="140" />
            <el-table-column label="通道" prop="channel_tag" width="80" />
            <el-table-column label="类型" width="120">
              <template #default="{ row }">
                <el-tag :type="row.policy_type === 'Timed' ? '' : 'warning'" size="small">
                  {{ row.policy_type === 'Timed' ? '定时录制' : '报警触发' }}
                </el-tag>
              </template>
            </el-table-column>
            <el-table-column label="时间计划" min-width="180">
              <template #default="{ row }">
                <span v-if="row.policy_type === 'Timed' && row.schedule_type === 'TimeWindows'">
                  <span v-for="(tw, i) in row.time_windows" :key="i" class="time-window-tag">
                    {{ tw.days?.map(d => ['日','一','二','三','四','五','六'][d] || d).join('') || '每天' }} {{ tw.start_time }}-{{ tw.end_time }}
                  </span>
                </span>
                <span v-else-if="row.cron_expr">{{ row.cron_expr }}</span>
                <span v-else>-</span>
              </template>
            </el-table-column>
            <el-table-column label="报警类型" width="100">
              <template #default="{ row }">
                <span v-if="row.policy_type === 'AlarmTriggered'">{{ row.alarm_type || '-' }}</span>
                <span v-else>-</span>
              </template>
            </el-table-column>
            <el-table-column label="预录/后录" width="110">
              <template #default="{ row }">
                <span v-if="row.policy_type === 'AlarmTriggered'">
                  {{ row.pre_record_secs || 0 }}s / {{ row.post_record_secs || 0 }}s
                </span>
                <span v-else>-</span>
              </template>
            </el-table-column>
            <el-table-column label="格式" prop="format" width="80" />
            <el-table-column label="创建时间" width="160">
              <template #default="{ row }"><span>{{ formatDate(row.created_at) }}</span></template>
            </el-table-column>
            <el-table-column label="操作" width="140">
              <template #default="{ row }">
                <el-button size="small" @click="openEditDialog(row)">编辑</el-button>
                <el-button size="small" type="danger" plain @click="confirmDelete(row)">删除</el-button>
              </template>
            </el-table-column>
          </el-table>
          <el-empty v-else description="暂无录制策略，点击上方按钮创建" :image-size="60" />
        </el-skeleton>
      </DataCard>
    </div>

    <el-dialog v-model="showDialog" :title="isEditing ? '编辑录制策略' : '新建录制策略'" width="600px" destroy-on-close>
      <el-form :model="form" label-position="top" :rules="formRules" ref="formRef">
        <el-form-item label="设备 *" prop="device_tag">
          <el-select
            v-model="form.device_tag"
            placeholder="选择设备"
            filterable
            remote
            :remote-method="searchDevices"
            :loading="deviceLoading"
            style="width: 100%"
            @change="onDeviceChange"
          >
            <el-option
              v-for="d in deviceOptions"
              :key="d.device_tag"
              :value="d.device_tag"
              :label="d.name || d.device_tag"
            >
              <div style="display: flex; align-items: center; gap: 6px">
                <span class="status-dot" :class="d.status === 'Online' ? 'online' : 'offline'" style="width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0"></span>
                <span style="flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">{{ d.name || d.device_tag }}</span>
              </div>
            </el-option>
          </el-select>
        </el-form-item>

        <el-form-item label="通道 *" prop="channel_tag">
          <el-select v-model="form.channel_tag" placeholder="选择通道" style="width: 100%">
            <el-option
              v-for="ch in channelOptions"
              :key="ch.channel_tag"
              :value="ch.channel_tag"
              :label="`通道 ${ch.channel_tag} - ${ch.name || ch.channel_tag || '未命名'}`"
            />
          </el-select>
        </el-form-item>

        <el-form-item label="策略类型 *" prop="policy_type">
          <el-radio-group v-model="form.policy_type" @change="onPolicyTypeChange">
            <el-radio value="Timed">定时录制</el-radio>
            <el-radio value="AlarmTriggered">报警触发录制</el-radio>
          </el-radio-group>
        </el-form-item>

        <el-form-item label="启用状态">
          <el-switch v-model="form.enabled" />
        </el-form-item>

        <template v-if="form.policy_type === 'Timed'">
          <el-form-item label="时间计划类型">
            <el-radio-group v-model="form.schedule_type">
              <el-radio value="TimeWindows">时间窗口</el-radio>
              <el-radio value="Cron">Cron表达式</el-radio>
            </el-radio-group>
          </el-form-item>

          <el-form-item v-if="form.schedule_type === 'TimeWindows'" label="时间窗口" prop="time_windows">
            <div class="time-windows-list">
              <div v-for="(tw, i) in form.time_windows" :key="i" class="time-window-row">
                <el-select v-model="tw.days" multiple placeholder="选择日期" style="width: 200px">
                  <el-option v-for="d in weekDays" :key="d.value" :value="d.value" :label="d.label" />
                </el-select>
                <el-time-select v-model="tw.start_time" placeholder="开始时间" style="width: 120px" />
                <el-time-select v-model="tw.end_time" placeholder="结束时间" style="width: 120px" />
                <el-button text type="danger" @click="removeTimeWindow(i)">删除</el-button>
              </div>
              <el-button @click="addTimeWindow">+ 添加时间窗口</el-button>
            </div>
            <div class="template-buttons">
              <el-button size="small" @click="applyTemplate('24h')">24小时连续录制</el-button>
              <el-button size="small" @click="applyTemplate('workday')">工作日白天 (08:00-18:00)</el-button>
              <el-button size="small" @click="applyTemplate('night')">夜间警戒 (22:00-06:00)</el-button>
            </div>
          </el-form-item>

          <el-form-item v-else label="Cron表达式" prop="cron_expr">
            <el-input v-model="form.cron_expr" placeholder="0 20 * * 1,3,5" />
            <div style="font-size: 12px; color: #999; margin-top: 4px">格式: 分 时 日 月 周 (例: "0 20 * * 1,3,5" 表示周一三五20:00开始)</div>
          </el-form-item>
        </template>

        <template v-else>
          <el-form-item label="报警类型" prop="alarm_type">
            <el-select v-model="form.alarm_type" placeholder="选择报警类型" style="width: 100%">
              <el-option value="motion" label="运动检测 (motion)" />
              <el-option value="external" label="外部报警 (external)" />
              <el-option value="custom" label="自定义" />
            </el-select>
          </el-form-item>

          <el-form-item v-if="form.alarm_type === 'custom'" label="自定义报警类型" prop="custom_alarm_type">
            <el-input v-model="form.custom_alarm_type" placeholder="输入自定义报警类型" />
          </el-form-item>

          <el-row :gutter="12">
            <el-col :span="12">
              <el-form-item label="预录秒数">
                <el-input-number v-model="form.pre_record_secs" :min="0" :max="300" style="width: 100%" />
              </el-form-item>
            </el-col>
            <el-col :span="12">
              <el-form-item label="后录秒数">
                <el-input-number v-model="form.post_record_secs" :min="0" :max="3600" style="width: 100%" />
              </el-form-item>
            </el-col>
          </el-row>
        </template>

        <el-form-item label="录像格式">
          <el-select v-model="form.format" style="width: 100%">
            <el-option value="Mp4" label="MP4" />
            <el-option value="Hls" label="HLS" />
            <el-option value="Flv" label="FLV" />
            <el-option value="Ts" label="TS" />
          </el-select>
        </el-form-item>

        <el-row :gutter="12">
          <el-col :span="12">
            <el-form-item label="最大时长 (秒)">
              <el-input-number v-model="form.max_duration_secs" :min="0" placeholder="0=不限制" style="width: 100%" />
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="最大文件 (MB)">
              <el-input-number v-model="form.max_file_size_mb" :min="0" placeholder="0=不限制" style="width: 100%" />
            </el-form-item>
          </el-col>
        </el-row>
      </el-form>
      <template #footer>
        <el-button @click="showDialog = false">取消</el-button>
        <el-button type="primary" :loading="submitting" @click="submitForm">确定</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, shallowRef } from 'vue'
import { ElMessageBox, ElMessage } from 'element-plus'
import { Refresh, Plus } from '@element-plus/icons-vue'
import { useRecordingStore } from '../stores/recordingStore'
import { useToast } from '../composables/useToast'
import { request } from '../utils/request'
import DataCard from '../components/common/DataCard.vue'

const recStore = useRecordingStore()
const toast = useToast()

const loading = computed(() => recStore.policyLoading)
const policies = computed(() => recStore.policies)

const showDialog = ref(false)
const isEditing = ref(false)
const submitting = ref(false)
const formRef = ref(null)
const editingId = ref(null)

const deviceOptions = shallowRef([])
const channelOptions = ref([])
const deviceLoading = ref(false)

const weekDays = [
  { value: 0, label: '周日' },
  { value: 1, label: '周一' },
  { value: 2, label: '周二' },
  { value: 3, label: '周三' },
  { value: 4, label: '周四' },
  { value: 5, label: '周五' },
  { value: 6, label: '周六' },
]

const defaultForm = () => ({
  device_tag: '',
  channel_tag: null,
  policy_type: 'Timed',
  enabled: true,
  schedule_type: 'TimeWindows',
  cron_expr: '',
  time_windows: [],
  alarm_type: 'motion',
  custom_alarm_type: '',
  pre_record_secs: 0,
  post_record_secs: 30,
  format: 'Mp4',
  max_duration_secs: 3600,
  max_file_size_mb: 512,
})

const form = ref(defaultForm())

const validateTimeWindow = (rule, value, callback) => {
  if (!value || value.length === 0) {
    callback()
    return
  }
  for (let i = 0; i < value.length; i++) {
    const tw = value[i]
    if (!tw.start_time || !tw.end_time) {
      callback(new Error('请填写时间窗口的开始和结束时间'))
      return
    }
    if (tw.start_time >= tw.end_time) {
      callback(new Error(`第${i + 1}个时间窗口: 开始时间必须早于结束时间`))
      return
    }
  }
  callback()
}

const formRules = {
  device_tag: [{ required: true, message: '请选择设备', trigger: 'change' }],
  channel_tag: [{ required: true, message: '请选择通道', trigger: 'change' }],
  policy_type: [{ required: true, message: '请选择策略类型', trigger: 'change' }],
  cron_expr: [{ required: true, message: '请输入Cron表达式', trigger: 'blur' }],
  time_windows: [{ validator: validateTimeWindow, trigger: 'change' }],
}

onMounted(() => {
  fetchPolicies()
  searchDevices('')
})

async function fetchPolicies() {
  try {
    await recStore.fetchPolicies()
  } catch (e) {
    toast.error('加载策略失败: ' + e.message)
  }
}

async function searchDevices(query) {
  if (deviceLoading.value) return
  deviceLoading.value = true
  try {
    const res = await request.get('/devices', {
      params: { limit: 50, offset: 0, search: query || undefined },
    })
    deviceOptions.value = res.data?.data?.items || []
  } catch (e) {
    // silent
  } finally {
    deviceLoading.value = false
  }
}

async function onDeviceChange(deviceTag, preserveChannel = false) {
  if (!preserveChannel) {
    form.value.channel_tag = null
  }
  channelOptions.value = []
  if (!deviceTag) return
  try {
    const res = await request.get('/channels', {
      params: { device_tag: deviceTag, limit: 100, offset: 0 },
    })
    channelOptions.value = res.data?.data?.items || []
  } catch (e) {
    // silent
  }
}

function onPolicyTypeChange() {
  form.value.time_windows = []
  form.value.cron_expr = ''
}

function addTimeWindow() {
  form.value.time_windows.push({
    days: [0, 1, 2, 3, 4, 5, 6],
    start_time: '08:00',
    end_time: '18:00',
  })
}

function removeTimeWindow(index) {
  form.value.time_windows.splice(index, 1)
}

function applyTemplate(template) {
  if (template === '24h') {
    form.value.time_windows = [{
      days: [0, 1, 2, 3, 4, 5, 6],
      start_time: '00:00',
      end_time: '23:59',
    }]
  } else if (template === 'workday') {
    form.value.time_windows = [{
      days: [1, 2, 3, 4, 5],
      start_time: '08:00',
      end_time: '18:00',
    }]
  } else if (template === 'night') {
    form.value.time_windows = [{
      days: [0, 1, 2, 3, 4, 5, 6],
      start_time: '22:00',
      end_time: '06:00',
    }]
  }
}

function openCreateDialog() {
  isEditing.value = false
  editingId.value = null
  form.value = defaultForm()
  deviceOptions.value = []
  channelOptions.value = []
  showDialog.value = true
  searchDevices('')
}

function openEditDialog(row) {
  isEditing.value = true
  editingId.value = row.id
  form.value = {
    device_tag: row.device_tag,
    channel_tag: row.channel_tag,
    policy_type: row.policy_type,
    enabled: row.enabled,
    schedule_type: row.schedule_type || 'TimeWindows',
    cron_expr: row.cron_expr || '',
    time_windows: row.time_windows ? [...row.time_windows] : [],
    alarm_type: row.alarm_type || 'motion',
    custom_alarm_type: '',
    pre_record_secs: row.pre_record_secs || 0,
    post_record_secs: row.post_record_secs || 30,
    format: row.format || 'Mp4',
    max_duration_secs: row.max_duration_secs || 3600,
    max_file_size_mb: row.max_file_size_mb || 512,
  }
  // 载入当前设备到下拉选项，否则下拉框只能显示 device_tag 原始值而非设备名
  ensureDeviceOption(row.device_tag)
  onDeviceChange(row.device_tag, true).then(() => {
    showDialog.value = true
  })
  showDialog.value = true
}

/** 确保指定设备存在于设备下拉选项中（编辑回填时使用） */
async function ensureDeviceOption(deviceTag) {
  if (!deviceTag) return
  if (deviceOptions.value.some((d) => d.device_tag === deviceTag)) return
  try {
    const res = await request.get('/devices', {
      params: { limit: 50, offset: 0, search: deviceTag },
    })
    const items = res.data?.data?.items || []
    const found = items.find((d) => d.device_tag === deviceTag)
    if (found) {
      deviceOptions.value = [found, ...deviceOptions.value]
    }
  } catch (e) {
    // silent
  }
}

async function submitForm() {
  try {
    await formRef.value?.validate()
  } catch {
    return
  }

  submitting.value = true
  try {
    const data = {
      device_tag: form.value.device_tag,
      channel_tag: form.value.channel_tag,
      policy_type: form.value.policy_type,
      enabled: form.value.enabled,
      schedule_type: form.value.schedule_type,
      cron_expr: form.value.cron_expr || null,
      time_windows: form.value.time_windows,
      alarm_type: form.value.policy_type === 'AlarmTriggered'
        ? (form.value.alarm_type === 'custom' ? form.value.custom_alarm_type : form.value.alarm_type)
        : null,
      pre_record_secs: form.value.pre_record_secs,
      post_record_secs: form.value.post_record_secs,
      format: form.value.format,
      max_duration_secs: form.value.max_duration_secs,
      max_file_size_mb: form.value.max_file_size_mb,
    }

    if (isEditing.value) {
      await recStore.updatePolicy(editingId.value, data)
    } else {
      await recStore.createPolicy(data)
    }
    showDialog.value = false
    await fetchPolicies()
  } catch (e) {
    toast.error((isEditing.value ? '更新' : '创建') + '策略失败: ' + e.message)
  } finally {
    submitting.value = false
  }
}

async function confirmDelete(row) {
  try {
    await ElMessageBox.confirm(`确定删除策略 "${row.device_tag} - 通道${row.channel_tag}" 吗？`, '确认删除', { type: 'warning' })
    await recStore.deletePolicy(row.id)
    await fetchPolicies()
  } catch {}
}

function formatDate(dateStr) {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleString('zh-CN', { hour12: false })
}
</script>

<style scoped>
.time-windows-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.time-window-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.time-window-tag {
  display: inline-block;
  background: var(--bg-elevated);
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 12px;
  margin-right: 4px;
}
.template-buttons {
  margin-top: 8px;
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
