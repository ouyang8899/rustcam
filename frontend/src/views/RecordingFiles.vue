<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">录像文件</h1>
      <div class="page-toolbar">
        <el-button :icon="Refresh" :loading="loading" @click="fetchFiles">刷新</el-button>
      </div>
    </div>
    <div class="page-body" v-loading="loading" element-loading-text="加载中...">
      <DataCard>
        <template #header>
          <span class="data-card__title">录像文件列表</span>
          <div class="data-card__toolbar">
            <el-select v-model="filters.device_tag" clearable placeholder="选择设备" style="width: 150px" @change="onDeviceChange">
              <el-option v-for="d in deviceOptions" :key="d.device_tag" :value="d.device_tag" :label="d.name || d.device_tag" />
            </el-select>
            <el-select v-model="filters.channel_tag" clearable placeholder="选择通道" style="width: 120px" :disabled="!filters.device_tag" @change="onFilterChange">
              <el-option v-for="ch in channelOptions" :key="ch.channel_tag" :value="ch.channel_tag" :label="`通道 ${ch.channel_tag}`" />
            </el-select>
            <el-date-picker
              v-model="filters.dateRange"
              type="datetimerange"
              range-separator="至"
              start-placeholder="开始时间"
              end-placeholder="结束时间"
              value-format="YYYY-MM-DD HH:mm:ss"
              style="width: 340px"
              @change="onFilterChange"
            />
            <el-select v-model="filters.segment_type" clearable placeholder="全部类型" style="width: 130px" @change="onFilterChange">
              <el-option value="normal" label="普通录制" />
              <el-option value="alarm" label="报警触发" />
              <el-option value="prebuffer" label="预录段" />
            </el-select>
          </div>
        </template>
        <el-skeleton animated :loading="loading" :rows="5">
          <el-table :data="files" style="margin-top: 0" @row-dblclick="onRowDoubleClick">
            <el-table-column label="文件名" prop="filename" min-width="200" show-overflow-tooltip>
              <template #default="{ row }">
                <span class="filename-cell">{{ row.filename || row.file_path?.split('/').pop() || row.id }}</span>
              </template>
            </el-table-column>
            <el-table-column label="类型" width="100">
              <template #default="{ row }">
                <el-tag :type="segmentTypeTagType(row.segment_type)" size="small">{{ segmentTypeLabel(row.segment_type) }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column label="开始时间" width="160">
              <template #default="{ row }"><span>{{ formatDateTime(row.start_time) }}</span></template>
            </el-table-column>
            <el-table-column label="结束时间" width="160">
              <template #default="{ row }"><span>{{ formatDateTime(row.end_time) }}</span></template>
            </el-table-column>
            <el-table-column label="时长" width="90">
              <template #default="{ row }"><span>{{ formatDuration(row.duration_secs) }}</span></template>
            </el-table-column>
            <el-table-column label="大小" width="100">
              <template #default="{ row }"><span>{{ formatSize(row.file_size) }}</span></template>
            </el-table-column>
            <el-table-column label="媒体服务器" prop="server_tag" width="120" show-overflow-tooltip />
            <el-table-column label="操作" width="100">
              <template #default="{ row }">
                <el-button size="small" type="primary" plain @click="playFile(row)">播放</el-button>
              </template>
            </el-table-column>
          </el-table>
          <div v-if="hasMore" style="text-align: center; padding: 16px 0">
            <el-button :loading="loadingMore" @click="loadMore">加载更多</el-button>
          </div>
          <div v-else-if="files.length > 0" style="text-align: center; padding: 16px 0; color: #999">
            已加载全部 {{ files.length }} 条记录
          </div>
        </el-skeleton>
        <el-empty v-if="!loading && files.length === 0" description="暂无录像文件" />
      </DataCard>
    </div>

    <el-dialog v-model="showPlaybackDialog" title="录像回放" width="900px" destroy-on-close>
      <VideoPlayer
        ref="videoPlayerRef"
        :src="currentPlayUrl"
        :is-live="false"
        :autoplay="true"
        @ended="onPlaybackEnded"
        @timeupdate="onTimeUpdate"
      />
      <div v-if="playbackFiles.length > 1" class="playback-files-nav">
        <el-button
          v-for="(f, i) in playbackFiles"
          :key="f.id"
          size="small"
          :type="currentFileIndex === i ? 'primary' : ''"
          @click="switchToFile(i)"
        >
          {{ formatDateTime(f.start_time) }}
        </el-button>
      </div>
      <div class="playback-progress">
        <span>{{ formatDuration(currentTime) }}</span>
        <el-slider v-model="seekPosition" :min="0" :max="duration" @change="onSeek" />
        <span>{{ formatDuration(duration) }}</span>
      </div>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, shallowRef } from 'vue'
import { ElMessage } from 'element-plus'
import { Refresh } from '@element-plus/icons-vue'
import VideoPlayer from '../components/VideoPlayer.vue'
import { useRecordingStore } from '../stores/recordingStore'
import { useToast } from '../composables/useToast'
import { request } from '../utils/request'
import DataCard from '../components/common/DataCard.vue'

const recStore = useRecordingStore()
const toast = useToast()

const files = computed(() => recStore.recordingFiles)
const loading = computed(() => recStore.filesLoading)
const filesTotal = computed(() => recStore.filesTotal)
const filesPage = computed(() => recStore.filesPage)
const filesLimit = computed(() => recStore.filesLimit)
const hasMore = computed(() => files.value.length < filesTotal.value)

const deviceOptions = shallowRef([])
const channelOptions = ref([])
const deviceLoading = ref(false)

const filters = ref({
  device_tag: '',
  channel_tag: null,
  dateRange: null,
  segment_type: '',
})
const showPlaybackDialog = ref(false)
const currentPlayUrl = ref('')
const playbackFiles = ref([])
const currentFileIndex = ref(0)
const loadingMore = ref(false)
const videoPlayerRef = ref(null)
const currentTime = ref(0)
const duration = ref(0)
const seekPosition = ref(0)

const currentFile = computed(() => playbackFiles.value[currentFileIndex.value])

onMounted(() => {
  searchDevices('')
  fetchFiles()
})

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

async function onDeviceChange(deviceTag) {
  filters.value.channel_tag = null
  channelOptions.value = []
  if (!deviceTag) {
    onFilterChange()
    return
  }
  try {
    const res = await request.get('/channels', {
      params: { device_tag: deviceTag, limit: 100, offset: 0 },
    })
    channelOptions.value = res.data?.data?.items || []
  } catch (e) {
    // silent
  }
  onFilterChange()
}

async function fetchFiles(resetPage = true) {
  if (resetPage) {
    await recStore.fetchRecordingFiles({
      ...getFilterParams(),
      page: 1,
      limit: 50,
    })
  } else {
    await recStore.fetchRecordingFiles({
      ...getFilterParams(),
      page: recStore.filesPage,
      limit: recStore.filesLimit,
    })
  }
}

async function loadMore() {
  loadingMore.value = true
  try {
    await recStore.fetchRecordingFiles({
      ...getFilterParams(),
      page: recStore.filesPage + 1,
      limit: recStore.filesLimit,
    })
  } finally {
    loadingMore.value = false
  }
}

function getFilterParams() {
  const params = {}
  if (filters.value.device_tag) {
    params.device_tag = filters.value.device_tag
  }
  if (filters.value.channel_tag !== null && filters.value.channel_tag !== undefined) {
    params.channel_tag = filters.value.channel_tag
  }
  if (filters.value.dateRange && filters.value.dateRange.length === 2) {
    // 后端按 RFC3339 解析时间，日期选择器给的是 'YYYY-MM-DD HH:mm:ss'
    params.start_time = toApiTime(filters.value.dateRange[0])
    params.end_time = toApiTime(filters.value.dateRange[1])
  }
  if (filters.value.segment_type) {
    params.segment_type = filters.value.segment_type
  }
  return params
}

/** 'YYYY-MM-DD HH:mm:ss'（本地时间）-> ISO8601 UTC（RFC3339） */
function toApiTime(value) {
  if (!value) return value
  const normalized = String(value).replace(' ', 'T')
  const d = new Date(normalized)
  if (isNaN(d.getTime())) return value
  return d.toISOString()
}

function onFilterChange() {
  fetchFiles()
}

async function playFile(row) {
  playbackFiles.value = [row]
  currentFileIndex.value = 0
  await loadPlayUrl()
  showPlaybackDialog.value = true
}

async function onRowDoubleClick(row) {
  await playFile(row)
}

async function loadPlayUrl() {
  try {
    const playData = await recStore.getRecordingFilePlayUrl(currentFile.value.id)
    currentPlayUrl.value = playData.play_url || playData.url
  } catch (e) {
    currentPlayUrl.value = currentFile.value.play_url || currentFile.value.url
  }
}

function switchToFile(index) {
  currentFileIndex.value = index
  loadPlayUrl()
}

function onPlaybackEnded() {
  if (currentFileIndex.value < playbackFiles.value.length - 1) {
    switchToFile(currentFileIndex.value + 1)
  }
}

function onTimeUpdate(data) {
  if (data && data.currentTime !== undefined) {
    currentTime.value = data.currentTime
    duration.value = data.duration || 0
  }
}

function onSeek(value) {
  if (videoPlayerRef.value && videoPlayerRef.value.seek) {
    videoPlayerRef.value.seek(value)
  }
}

function segmentTypeTagType(type) {
  const map = { normal: '', alarm: 'warning', prebuffer: 'info' }
  return map[type] || ''
}

function segmentTypeLabel(type) {
  const map = { normal: '普通', alarm: '报警', prebuffer: '预录' }
  return map[type] || type || '-'
}

function formatDuration(secs) {
  if (!secs || isNaN(secs)) return '00:00'
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = Math.floor(secs % 60)
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  return `${m}:${String(s).padStart(2, '0')}`
}

function formatSize(bytes) {
  if (!bytes) return '-'
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(0)} KB`
  return `${bytes} B`
}

function formatDateTime(ts) {
  if (!ts) return '-'
  const d = new Date(ts)
  if (isNaN(d.getTime())) return ts
  return d.toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false
  })
}
</script>

<style scoped>
.playback-files-nav {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  padding: 12px 0;
  justify-content: center;
}
.playback-progress {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 0;
  font-size: 12px;
  color: #666;
}
.playback-progress .el-slider {
  flex: 1;
}
.filename-cell {
  font-family: monospace;
  font-size: 12px;
}
</style>
