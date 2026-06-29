<template>
  <div >
    <div v-if="!src && !liveSrc">
      <div>
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"></path>
        </svg>
        <p>No stream URL</p>
      </div>
    </div>

    <div v-else >
      <video
        ref="videoRef"
       
        
        @click="togglePlay"
        @dblclick="toggleFullscreen"
      ></video>

      <div v-if="loading">
        <div></div>
      </div>

      <div v-if="error">
        <div>
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
          </svg>
          <p>{{ error }}</p>
          <button @click="reconnect">
            Retry
          </button>
        </div>
      </div>

      <div class="controls">
        <div>
          <div>
            <button @click="togglePlay">
              <svg v-if="isPlaying" fill="currentColor" viewBox="0 0 24 24">
                <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>
              </svg>
              <svg v-else fill="currentColor" viewBox="0 0 24 24">
                <path d="M8 5v14l11-7z"/>
              </svg>
            </button>

            <div v-if="currentTime > 0">
              {{ formatTime(currentTime) }} / {{ formatTime(duration) }}
            </div>
          </div>

          <div>
            <div v-if="isLive" class="live-tag">
              LIVE
            </div>

            <div v-if="bitrate">
              {{ bitrate }} kbps
            </div>

            <button @click="toggleFullscreen">
              <svg v-if="isFullscreen" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h-4.5M15 15v4.5m0-4.5l5.25 5.25"/>
              </svg>
              <svg v-else fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5v-4m0 4h-4m4 0l-5-5"/>
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, watch, onMounted, onUnmounted, computed } from 'vue'
import Hls from 'hls.js'
import flvjs from 'flv.js'

const props = defineProps({
  src: String,
  liveSrc: String,
  type: {
    type: String,
    default: 'auto'
  },
  isLive: {
    type: Boolean,
    default: false
  },
  aspectRatio: {
    type: String,
    default: '16/9'
  },
  autoplay: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['play', 'pause', 'error', 'ended', 'stats'])

const videoRef = ref(null)
const loading = ref(false)
const error = ref(null)
const isPlaying = ref(false)
const isVideoReady = ref(false)
const isFullscreen = ref(false)
const currentTime = ref(0)
const duration = ref(0)
const bitrate = ref(0)
const hideControls = ref(false)
const hideControlsTimer = ref(null)

let hlsInstance = null
let flvInstance = null
let wsInstance = null
let mediaSource = null
let mseVideoBuffer = null
let mseAudioBuffer = null
let mseReady = false
let wsAccumulator = null
let wsAccumulatorLen = 0
let wsConnected = false
let flvHeaderParsed = false
let videoCodec = null
let audioCodec = null

const currentSrc = computed(() => props.liveSrc || props.src)

function detectProtocol(url) {
  if (!url) return 'unknown'
  if (url.includes('.m3u8')) return 'hls'
  if (url.includes('.flv')) return 'flv'
  if (url.includes('ws://') || url.includes('wss://')) return 'wsflv'
  if (url.includes('webrtc')) return 'webrtc'
  return 'unknown'
}

async function initPlayer() {
  const url = currentSrc.value
  if (!url) return

  destroyPlayer()

  loading.value = true
  error.value = null
  isVideoReady.value = false

  const protocol = props.type === 'auto' ? detectProtocol(url) : props.type
  console.log('[VideoPlayer] initPlayer:', { url, propsType: props.type, detectedProtocol: protocol })

  try {
    switch (protocol) {
      case 'hls':
        await initHls(url)
        break
      case 'flv':
        await initFlv(url)
        break
      case 'wsflv':
        await initWsFlv(url)
        break
      default:
        error.value = 'Unsupported protocol'
        break
    }
  } catch (e) {
    error.value = e.message || 'Failed to initialize player'
    loading.value = false
  }
}

async function initHls(url) {
  const video = videoRef.value
  if (Hls.isSupported()) {
    hlsInstance = new Hls({
      enableWorker: true,
      lowLatencyMode: true,
      backBufferLength: 90
    })

    hlsInstance.loadSource(url)
    hlsInstance.attachMedia(video)

    hlsInstance.on(Hls.Events.MANIFEST_PARSED, () => {
      loading.value = false
      isVideoReady.value = true
      if (props.autoplay || props.isLive) {
        video.play().catch(() => {})
      }
    })

    hlsInstance.on(Hls.Events.ERROR, (event, data) => {
      if (data.fatal) {
        error.value = `HLS Error: ${data.type}`
        emit('error', error.value)
      }
    })

    hlsInstance.on(Hls.Events.FRAG_LOADED, () => {
      updateStats()
    })
  } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
    video.src = url
    video.addEventListener('loadedmetadata', () => {
      loading.value = false
      isVideoReady.value = true
    })
  } else {
    error.value = 'HLS not supported'
  }
}

async function initFlv(url) {
  console.log('[VideoPlayer] initFlv called with:', url)
  if (flvjs.isSupported()) {
    flvInstance = flvjs.createPlayer({
      type: 'flv',
      url: url,
      hasAudio: true,
      hasVideo: true,
      isLive: props.isLive
    }, {
      enableWorker: false,
      enableStashBuffer: false,
      stashInitialSize: 128
    })

    flvInstance.attachMediaElement(videoRef.value)
    flvInstance.load()

    flvInstance.on(flvjs.Events.LOADING_COMPLETE, () => {
      console.log('[VideoPlayer] FLV LOADING_COMPLETE')
      loading.value = false
      isVideoReady.value = true
      if (props.autoplay || props.isLive) {
        videoRef.value?.play().catch(() => {})
      }
    })

    flvInstance.on(flvjs.Events.ERROR, (errType, errDetail) => {
      console.error('[VideoPlayer] FLV ERROR:', errType, errDetail)
      error.value = `FLV Error: ${errDetail}`
      emit('error', error.value)
    })

    flvInstance.on(flvjs.Events.STATISTICS_INFO, (info) => {
      bitrate.value = Math.round(info.speed / 1000)
    })
  } else {
    console.error('[VideoPlayer] FLV not supported')
    error.value = 'FLV not supported'
  }
}

async function initWsFlv(url) {
  console.log('[VideoPlayer] initWsFlv called with:', url)
  if (flvjs.isSupported()) {
    flvInstance = flvjs.createPlayer({
      type: 'flv',
      url: url,
      hasAudio: true,
      hasVideo: true,
      isLive: props.isLive
    }, {
      enableWorker: false,
      enableStashBuffer: false,
      stashInitialSize: 128
    })

    flvInstance.attachMediaElement(videoRef.value)
    flvInstance.load()

    flvInstance.on(flvjs.Events.LOADING_COMPLETE, () => {
      console.log('[VideoPlayer] FLV LOADING_COMPLETE')
      loading.value = false
      isVideoReady.value = true
    })

    flvInstance.on(flvjs.Events.ERROR, (errType, errDetail) => {
      console.error('[VideoPlayer] FLV ERROR:', errType, errDetail)
      error.value = `FLV Error: ${errDetail}`
      emit('error', error.value)
    })

    flvInstance.on(flvjs.Events.STATISTICS_INFO, (info) => {
      bitrate.value = Math.round(info.speed / 1000)
    })

    loading.value = false
    isVideoReady.value = true
  } else {
    console.error('[VideoPlayer] FLV not supported')
    error.value = 'FLV not supported'
    loading.value = false
  }
}

function destroyPlayer() {
  if (hlsInstance) {
    hlsInstance.destroy()
    hlsInstance = null
  }
  if (flvInstance) {
    flvInstance.pause()
    flvInstance.unload()
    flvInstance.detachMediaElement()
    flvInstance.destroy()
    flvInstance = null
  }
  if (wsInstance) {
    wsInstance.close()
    wsInstance = null
  }
  wsConnected = false
  wsAccumulator = null
  wsAccumulatorLen = 0
  flvHeaderParsed = false
  if (mediaSource) {
    if (mseVideoBuffer) {
      try { mseVideoBuffer.abort() } catch (e) {}
      mseVideoBuffer = null
    }
    if (mseAudioBuffer) {
      try { mseAudioBuffer.abort() } catch (e) {}
      mseAudioBuffer = null
    }
    try { mediaSource.endOfStream() } catch (e) {}
    mediaSource = null
  }
  mseReady = false
}

function togglePlay() {
  const video = videoRef.value
  if (!video) return

  if (isPlaying.value) {
    video.pause()
    isPlaying.value = false
    emit('pause')
  } else {
    video.play().then(() => {
      isPlaying.value = true
      emit('play')
    }).catch(() => {})
  }
}

function toggleFullscreen() {
  const player = videoRef.value?.closest('.video-player')
  if (!player) return

  if (!document.fullscreenElement) {
    player.requestFullscreen?.()
    isFullscreen.value = true
  } else {
    document.exitFullscreen?.()
    isFullscreen.value = false
  }
}

function reconnect() {
  error.value = null
  initPlayer()
}

function updateStats() {
  if (hlsInstance) {
    const levels = hlsInstance.levels
    if (levels && levels.length > 0) {
      const level = levels[hlsInstance.currentLevel]
      if (level) {
        bitrate.value = Math.round(level.bitrate / 1000)
      }
    }
  }
  emit('stats', {
    bitrate: bitrate.value,
    currentTime: currentTime.value,
    duration: duration.value
  })
}

function resetHideControlsTimer() {
  hideControls.value = false
  clearTimeout(hideControlsTimer.value)
  hideControlsTimer.value = setTimeout(() => {
    if (isPlaying.value) {
      hideControls.value = true
    }
  }, 3000)
}

function formatTime(seconds) {
  if (!seconds || isNaN(seconds)) return '00:00'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = Math.floor(seconds % 60)
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
  }
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

watch(currentSrc, () => {
  initPlayer()
})

watch(() => props.isLive, (live) => {
  if (live && isPlaying.value) {
    hideControls.value = true
  }
})

onMounted(() => {
  const video = videoRef.value
  if (video) {
    video.addEventListener('play', () => {
      isPlaying.value = true
      emit('play')
      resetHideControlsTimer()
    })

    video.addEventListener('pause', () => {
      isPlaying.value = false
      emit('pause')
      hideControls.value = false
    })

    video.addEventListener('ended', () => {
      isPlaying.value = false
      emit('ended')
    })

    video.addEventListener('timeupdate', () => {
      currentTime.value = video.currentTime
    })

    video.addEventListener('durationchange', () => {
      duration.value = video.duration
    })

    video.addEventListener('waiting', () => {
      loading.value = true
    })

    video.addEventListener('canplay', () => {
      loading.value = false
    })

    video.addEventListener('mousemove', () => {
      if (isPlaying.value) {
        resetHideControlsTimer()
      }
    })
  }

  if (currentSrc.value) {
    initPlayer()
  }
})

onUnmounted(() => {
  clearTimeout(hideControlsTimer.value)
  destroyPlayer()
})

defineExpose({
  play: togglePlay,
  pause: togglePlay,
  reconnect,
  destroy: destroyPlayer
})
</script>

<style scoped>
video {
  max-width: 100%;
  width: 100%;
  height: auto;
  display: block;
}

.controls {
  background: rgba(0, 0, 0, 0.7);
  padding: 8px 12px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.controls > div {
  display: flex;
  align-items: center;
  gap: 12px;
}

.controls button {
  background: rgba(255, 255, 255, 0.2);
  border: none;
  border-radius: 4px;
  padding: 6px 8px;
  cursor: pointer;
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
}

.controls button:hover {
  background: rgba(255, 255, 255, 0.3);
}

.controls button svg {
  width: 16px;
  height: 16px;
}

.controls > div > div {
  color: white;
  font-size: 12px;
  font-family: Arial, sans-serif;
}

.controls .live-tag {
  background: #e74c3c;
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: bold;
  margin-left: 8px;
}
</style>


