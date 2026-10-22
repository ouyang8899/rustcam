<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">录像回放</h1>
      <div class="page-toolbar">
        <el-button :icon="Refresh" :loading="loading" @click="refresh">刷新</el-button>
      </div>
    </div>
    <div class="page-body">
      <DataCard>
        <template #header>
          <span class="data-card__title">选择录像时间段</span>
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
              style="width: 360px"
              @change="onFilterChange"
            />
            <el-button type="primary" @click="searchRecordings">查询</el-button>
          </div>
        </template>

        <div v-if="!currentFile" class="empty-state">
          <p>请选择设备、通道和时间范围进行查询</p>
        </div>

        <div v-else class="playback-container">
          <div class="video-wrapper">
            <VideoPlayer
              ref="videoPlayerRef"
              :src="currentPlayUrl"
              :is-live="false"
              :autoplay="true"
              @ended="onPlaybackEnded"
              @timeupdate="onTimeUpdate"
            />
          </div>

          <div class="playback-info">
            <div class="file-info">
              <span class="label">当前文件:</span>
              <span>{{ formatDateTime(currentFile.start_time) }} - {{ formatDateTime(currentFile.end_time) }}</span>
              <el-tag :type="segmentTypeTagType(currentFile.segment_type)" size="small">{{ segmentTypeLabel(currentFile.segment_type) }}</el-tag>
            </div>

            <div class="playback-progress">
              <span>{{ formatDuration(currentTime) }}</span>
              <el-slider v-model="seekPosition" :min="0" :max="duration || 100" @change="onSeek" />
              <span>{{ formatDuration(duration) }}</span>
            </div>

            <div class="playback-controls">
              <el-button-group>
                <el-button @click="togglePlay">
                  {{ isPlaying ? '暂停' : '播放' }}
                </el-button>
                <el-button @click="skipBackward">-10s</el-button>
                <el-button @click="skipForward">+10s</el-button>
              </el-button-group>
              <el-select v-model="playbackRate" style="width: 100px" @change="onRateChange">
                <el-option :value="0.5" label="0.5x" />
                <el-option :value="1" label="1x" />
                <el-option :value="1.5" label="1.5x" />
                <el-option :value="2" label="2x" />
              </el-select>
            </div>
          </div>

          <div v-if="files.length > 1" class="files-list">
            <div class="files-list-header">
              <span>录像片段 ({{ files.length }}个)</span>
            </div>
            <div class="files-list-body">
              <div
                v-for="(f, i) in files"
                :key="f.id"
                :class="['file-item', { active: currentFileIndex === i }]"
                @click="switchToFile(i)"
              >
                <span class="file-time">{{ formatDateTime(f.start_time) }}</span>
                <span class="file-duration">{{ formatDuration(f.duration_secs) }}</span>
                <el-tag :type="segmentTypeTagType(f.segment_type)" size="small">{{ segmentTypeLabel(f.segment_type) }}</el-tag>
              </div>
            </div>
          </div>
        </div>
      </DataCard>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, shallowRef } from 'vue'
import { Refresh } from '@element-plus/icons-vue'
import VideoPlayer from '../components/VideoPlayer.vue'
import { useRecordingStore } from '../stores/recordingStore'
import { useToast } from '../composables/useToast'
import { request } from '../utils/request'
import DataCard from '../components/common/DataCard.vue'

const recStore = useRecordingStore()
const toast = useToast()

const deviceOptions = shallowRef([])
const channelOptions = ref([])
const deviceLoading = ref(false)

const filters = ref({
  device_tag: '',
  channel_tag: null,
  dateRange: null,
})

const files = ref([])
const currentFileIndex = ref(0)
const loading = ref(false)
const videoPlayerRef = ref(null)
const currentTime = ref(0)
const duration = ref(0)
const seekPosition = ref(0)
const playbackRate = ref(1)
const isPlaying = ref(true)

const currentFile = computed(() => files.value[currentFileIndex.value])
const currentPlayUrl = computed(() => currentFile.value?.play_url || currentFile.value?.url || '')

onMounted(() => {
  searchDevices('')
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

function onFilterChange() {}

function segmentTypeTagType(type) {
  const map = { normal: '', alarm: 'warning', prebuffer: 'info' }
  return map[type] || ''
}

function segmentTypeLabel(type) {
  const map = { normal: '普通', alarm: '报警', prebuffer: '预录' }
  return map[type] || type || '-'
}

/** 'YYYY-MM-DD HH:mm:ss'（本地时间）-> ISO8601 UTC（RFC3339） */
function toApiTime(value) {
  if (!value) return value
  const normalized = String(value).replace(' ', 'T')
  const d = new Date(normalized)
  if (isNaN(d.getTime())) return value
  return d.toISOString()
}

async function searchRecordings() {
  if (!filters.value.device_tag) {
    toast.warning('请选择设备')
    return
  }
  if (!filters.value.dateRange || filters.value.dateRange.length < 2) {
    toast.warning('请选择时间范围')
    return
  }
  loading.value = true
  try {
    const params = {
      device_tag: filters.value.device_tag,
      start_time: toApiTime(filters.value.dateRange[0]),
      end_time: toApiTime(filters.value.dateRange[1]),
      limit: 100,
      page: 1,
    }
    if (filters.value.channel_tag !== null && filters.value.channel_tag !== undefined) {
      params.channel_tag = filters.value.channel_tag
    }
    const data = await recStore.fetchRecordingFiles(params)
    files.value = data.files || []
    currentFileIndex.value = 0
    if (files.value.length === 0) {
      toast.info('该时间范围内没有录像文件')
    }
  } catch (e) {
    toast.error('查询失败: ' + e.message)
  } finally {
    loading.value = false
  }
}

async function switchToFile(index) {
  currentFileIndex.value = index
  if (videoPlayerRef.value && videoPlayerRef.value.seek) {
    videoPlayerRef.value.seek(0)
  }
  videoPlayerRef.value?.setPlaybackRate?.(playbackRate.value)
}

function onPlaybackEnded() {
  if (currentFileIndex.value < files.value.length - 1) {
    switchToFile(currentFileIndex.value + 1)
  }
}

function onTimeUpdate(data) {
  if (data) {
    currentTime.value = data.currentTime || 0
    duration.value = data.duration || 0
    seekPosition.value = currentTime.value
  }
}

function onSeek(value) {
  if (videoPlayerRef.value && videoPlayerRef.value.seek) {
    videoPlayerRef.value.seek(value)
  }
}

function togglePlay() {
  if (videoPlayerRef.value) {
    videoPlayerRef.value.play()
    isPlaying.value = true
  }
}

function skipBackward() {
  const newTime = Math.max(0, currentTime.value - 10)
  onSeek(newTime)
}

function skipForward() {
  const newTime = Math.min(duration.value, currentTime.value + 10)
  onSeek(newTime)
}

function onRateChange(rate) {
  videoPlayerRef.value?.setPlaybackRate?.(rate)
}

function refresh() {
  if (files.value.length > 0) {
    searchRecordings()
  }
}

function formatDuration(secs) {
  if (!secs || isNaN(secs)) return '00:00'
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  const s = Math.floor(secs % 60)
  if (h > 0) return `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
  return `${m}:${String(s).padStart(2, '0')}`
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
.playback-container {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.video-wrapper {
  width: 100%;
  aspect-ratio: 16/9;
  background: #000;
  border-radius: 8px;
  overflow: hidden;
}
.playback-info {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.file-info {
  display: flex;
  align-items: center;
  gap: 12px;
}
.file-info .label {
  font-weight: 500;
}
.playback-progress {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
  color: #666;
}
.playback-progress .el-slider {
  flex: 1;
}
.playback-controls {
  display: flex;
  align-items: center;
  gap: 12px;
}
.files-list {
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}
.files-list-header {
  padding: 8px 12px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  font-size: 13px;
  font-weight: 500;
}
.files-list-body {
  max-height: 200px;
  overflow-y: auto;
}
.file-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  cursor: pointer;
  border-bottom: 1px solid var(--border);
  transition: background 0.15s;
}
.file-item:last-child {
  border-bottom: none;
}
.file-item:hover {
  background: var(--bg-elevated);
}
.file-item.active {
  background: var(--color-primary-light-9);
}
.file-time {
  font-size: 12px;
  font-family: monospace;
}
.file-duration {
  font-size: 12px;
  color: #999;
}
.empty-state {
  text-align: center;
  padding: 48px 0;
  color: #999;
}
</style>
