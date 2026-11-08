<!-- @ts-check -->
<template>
  <div v-if="visible && drone" class="drone-video-popup">
    <div class="popup-head">
      <div class="popup-title">
        <span class="popup-label">实时视频</span>
        <span class="popup-drone">{{ drone.name || drone.drone_tag }}</span>
      </div>
      <el-select
        v-if="cameras.length > 1"
        v-model="cameraIndex"
        size="small"
        class="popup-select"
      >
        <el-option v-for="(tag, i) in cameras" :key="tag" :label="tag" :value="i" />
      </el-select>
      <el-select
        v-if="channels.length > 1"
        v-model="channelIndex"
        size="small"
        class="popup-select"
      >
        <el-option
          v-for="(ch, i) in channels"
          :key="ch.channel_tag"
          :label="ch.name || ch.channel_tag"
          :value="i"
        />
      </el-select>
      <el-button text circle class="popup-close" @click="emit('close')">
        <el-icon><Close /></el-icon>
      </el-button>
    </div>
    <div class="popup-body">
      <VideoPlayer
        v-if="playUrl"
        :key="playUrl"
        :live-src="playUrl"
        type="auto"
        :is-live="true"
        :autoplay="true"
      />
      <div v-else class="popup-empty">
        <span v-if="loading">正在获取播放地址…</span>
        <span v-else-if="error">{{ error }}</span>
        <span v-else>未关联可播放视频</span>
        <el-button
          v-if="activeCameraTag && !loading"
          size="small"
          plain
          @click="resolveVideo"
        >重新获取</el-button>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, ref, watch } from 'vue'
import { Close } from '@element-plus/icons-vue'
import VideoPlayer from '../VideoPlayer.vue'
import { getDeviceChannels, getChannelPlayLinks } from '../../api/channels.js'
import { pickPlayUrl } from '../../utils/droneVideo.js'

const props = defineProps({
  drone: { type: Object, default: null },
  visible: { type: Boolean, default: false },
})

const emit = defineEmits(['close'])

const cameraIndex = ref(0)
const channelIndex = ref(0)
const channels = ref([])
const playUrl = ref('')
const loading = ref(false)
const error = ref('')
let requestSeq = 0

const cameras = computed(() => {
  const tags = props.drone?.camera_device_tags
  return Array.isArray(tags) ? tags.filter((tag) => typeof tag === 'string' && tag.length > 0) : []
})
const activeCameraTag = computed(() => cameras.value[cameraIndex.value] || '')
const activeChannel = computed(() => channels.value[channelIndex.value] || null)

watch(
  () => [props.visible, props.drone?.drone_tag],
  () => {
    if (props.visible) reset()
  },
  { immediate: true },
)

watch(cameraIndex, () => {
  channelIndex.value = 0
  loadChannels()
})

watch(channelIndex, () => {
  if (activeChannel.value) resolveVideo()
})

function reset() {
  cameraIndex.value = 0
  channelIndex.value = 0
  channels.value = []
  playUrl.value = ''
  error.value = ''
  loadChannels()
}

async function loadChannels() {
  const cameraTag = activeCameraTag.value
  channels.value = []
  playUrl.value = ''
  error.value = ''
  if (!cameraTag) return
  loading.value = true
  try {
    channels.value = (await getDeviceChannels(cameraTag)) || []
  } catch (e) {
    channels.value = []
  } finally {
    loading.value = false
  }
  if (channels.value.length > 0) {
    channelIndex.value = 0
    await resolveVideo()
  }
}

async function resolveVideo() {
  const cameraTag = activeCameraTag.value
  if (!cameraTag) return
  const channelTag = activeChannel.value?.channel_tag || cameraTag
  const seq = ++requestSeq
  loading.value = true
  error.value = ''
  playUrl.value = ''
  try {
    const data = await getChannelPlayLinks(cameraTag, channelTag)
    if (seq !== requestSeq) return
    const url = pickPlayUrl(data)
    if (!url) {
      error.value = '平台未返回播放地址，请确认通道已推流'
      return
    }
    playUrl.value = url
  } catch (e) {
    if (seq !== requestSeq) return
    error.value = '获取播放地址失败：' + (e.message || e)
  } finally {
    if (seq === requestSeq) loading.value = false
  }
}
</script>

<style scoped>
.drone-video-popup {
  position: absolute;
  right: 12px;
  bottom: 12px;
  width: 320px;
  z-index: 6;
  border-radius: 10px;
  overflow: hidden;
  background: rgba(8, 15, 30, 0.94);
  border: 1px solid var(--border);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
}
.popup-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
}
.popup-title {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}
.popup-label {
  font-size: 12px;
  color: var(--text-muted);
}
.popup-drone {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.popup-select {
  width: 110px;
}
.popup-close {
  flex: none;
}
.popup-body {
  height: 180px;
}
.popup-body :deep(.video-player) {
  height: 100%;
}
.popup-empty {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 12px;
  font-size: 12px;
  color: var(--text-muted);
  text-align: center;
}
@media (max-width: 1200px) {
  .drone-video-popup {
    width: 260px;
  }
  .popup-body {
    height: 150px;
  }
}
</style>
