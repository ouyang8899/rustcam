<!-- @ts-check -->
<template>
  <el-dialog
    :model-value="modelValue"
    title="选择摄像头"
    width="880px"
    destroy-on-close
    @update:model-value="$emit('update:modelValue', $event)"
    @open="onOpen"
  >
    <div class="camera-toolbar">
      <el-input
        v-model="search"
        clearable
        placeholder="搜索设备名称或 tag"
        style="width: 280px"
        @keyup.enter="loadDevices(1)"
        @clear="loadDevices(1)"
      />
      <el-button @click="loadDevices(1)">搜索</el-button>
      <span class="muted">已选 {{ localTags.length }} 个摄像头</span>
    </div>

    <el-table v-loading="loading" :data="devices" size="small" height="420" row-key="device_tag">
      <el-table-column type="expand">
        <template #default="{ row }">
          <div class="channel-list">
            <div class="channel-actions">
              <el-button size="small" @click="openAddChannel(row)">添加通道</el-button>
              <el-button size="small" text @click="reloadChannels(row)">刷新</el-button>
            </div>
            <el-empty
              v-if="!(channelsByDevice[row.device_tag] || []).length"
              description="暂无通道"
              :image-size="48"
            />
            <el-tag
              v-for="ch in channelsByDevice[row.device_tag] || []"
              :key="ch.channel_tag"
              size="small"
              effect="plain"
            >
              {{ ch.name || ch.channel_tag }} ({{ ch.channel_tag }})
            </el-tag>
          </div>
        </template>
      </el-table-column>
      <el-table-column label="设备" min-width="220">
        <template #default="{ row }">
          <div class="device-name">{{ row.name || row.device_tag }}</div>
          <div class="device-tag">{{ row.device_tag }}</div>
        </template>
      </el-table-column>
      <el-table-column prop="protocol" label="协议" width="110" />
      <el-table-column prop="status" label="状态" width="100" />
      <el-table-column label="操作" width="140" fixed="right">
        <template #default="{ row }">
          <el-button
            text
            :type="isSelected(row.device_tag) ? 'danger' : 'primary'"
            @click="toggle(row)"
          >
            {{ isSelected(row.device_tag) ? '取消关联' : '关联' }}
          </el-button>
        </template>
      </el-table-column>
    </el-table>

    <div class="pager">
      <el-pagination
        v-model:current-page="page"
        :page-size="pageSize"
        :total="total"
        layout="prev, pager, next, total"
        @current-change="loadDevices"
      />
    </div>

    <template #footer>
      <el-button @click="$emit('update:modelValue', false)">取消</el-button>
      <el-button type="primary" @click="confirm">确定</el-button>
    </template>

    <ChannelEditDialog
      v-model="channelDialogVisible"
      mode="create"
      :device-tag="channelDialogDeviceTag"
      :channel-tag="''"
      @saved="onChannelSaved"
    />
  </el-dialog>
</template>

<script setup>
import { ref } from 'vue'
import { ElMessage } from 'element-plus'
import { getDevices } from '../../api/devices'
import { getDeviceChannels } from '../../api/channels'
import ChannelEditDialog from '../channel/ChannelEditDialog.vue'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
  selectedTags: { type: Array, default: () => [] },
})

const emit = defineEmits(['update:modelValue', 'update:selectedTags'])

const search = ref('')
const loading = ref(false)
const devices = ref([])
const total = ref(0)
const page = ref(1)
const pageSize = 10
const localTags = ref([])
const channelsByDevice = ref({})
const channelDialogVisible = ref(false)
const channelDialogDeviceTag = ref('')

function isSelected(tag) {
  return localTags.value.includes(tag)
}

async function loadDevices(nextPage = page.value) {
  page.value = nextPage
  loading.value = true
  try {
    const res = await getDevices({
      limit: pageSize,
      offset: (page.value - 1) * pageSize,
      search: search.value.trim() || undefined,
    })
    devices.value = res?.items || []
    total.value = res?.total || 0
  } finally {
    loading.value = false
  }
}

function toggle(row) {
  const tag = row.device_tag
  if (!tag) return
  if (isSelected(tag)) {
    localTags.value = localTags.value.filter(t => t !== tag)
  } else {
    localTags.value = [...localTags.value, tag]
  }
}

async function loadChannels(row) {
  if (!row?.device_tag) return
  const list = await getDeviceChannels(row.device_tag)
  channelsByDevice.value = {
    ...channelsByDevice.value,
    [row.device_tag]: list || [],
  }
}

async function reloadChannels(row) {
  await loadChannels(row)
}

async function openAddChannel(row) {
  channelDialogDeviceTag.value = row.device_tag
  channelDialogVisible.value = true
  await loadChannels(row)
}

async function onChannelSaved() {
  const tag = channelDialogDeviceTag.value
  if (!tag) return
  const list = await getDeviceChannels(tag)
  channelsByDevice.value = {
    ...channelsByDevice.value,
    [tag]: list || [],
  }
  ElMessage.success('通道已创建')
}

function onOpen() {
  search.value = ''
  page.value = 1
  localTags.value = [...props.selectedTags]
  channelsByDevice.value = {}
  loadDevices(1)
}

function confirm() {
  emit('update:selectedTags', [...localTags.value])
  emit('update:modelValue', false)
}
</script>

<style scoped>
.camera-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
}
.muted {
  color: var(--text-muted);
  font-size: 12px;
}
.device-name {
  font-weight: 500;
}
.device-tag {
  font-size: 12px;
  color: var(--text-muted);
}
.channel-list {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
}
.channel-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
.pager {
  display: flex;
  justify-content: flex-end;
  margin-top: 12px;
}
</style>
