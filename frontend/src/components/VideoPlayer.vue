<template>
  <div class="video-player">
    <div v-if="!src && !liveSrc" class="empty-state-wrapper">
      <div>
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"></path>
        </svg>
        <p>No stream URL</p>
      </div>
    </div>

    <div v-else class="video-container">
      <video
        ref="videoRef"
        playsinline
        autoplay
        muted
        @click="togglePlay"
        @dblclick="toggleFullscreen"
      ></video>

      <div v-if="loading" class="overlay loading-overlay">
        <div class="spinner"></div>
      </div>

      <div v-if="error" class="overlay error-overlay">
        <div class="error-inner">
          <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"></path>
          </svg>
          <p>{{ error }}</p>
          <button @click="reconnect">Retry</button>
        </div>
      </div>

      <div class="controls" :class="{ hidden: hideControls }">
        <div class="controls-left">
          <button @click="togglePlay">
            <svg v-if="isPlaying" fill="currentColor" viewBox="0 0 24 24">
              <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>
            </svg>
            <svg v-else fill="currentColor" viewBox="0 0 24 24">
              <path d="M8 5v14l11-7z"/>
            </svg>
          </button>
          <div v-if="currentTime > 0" class="time-display">
            {{ formatTime(currentTime) }} / {{ formatTime(duration) }}
          </div>
        </div>

        <div class="controls-right">
          <div v-if="isLive" class="live-tag">LIVE</div>
          <div v-if="bitrate" class="bitrate-display">{{ bitrate }} kbps</div>
          <div v-if="enableBroadcast && deviceTag" class="broadcast-status">
            <span v-if="broadcastStatus === 'idle' && !isBroadcasting" class="status-text idle">可广播</span>
            <span v-else-if="broadcastStatus === 'connecting'" class="status-text connecting">连接中...</span>
            <span v-else-if="broadcastStatus === 'inviting'" class="status-text inviting">广播开启中...</span>
            <span v-else-if="broadcastStatus === 'ready'" class="status-text ready">通话中</span>
            <span v-else-if="broadcastStatus === 'error'" class="status-text error" :title="broadcastError">错误</span>
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

      <button
        v-if="enableBroadcast && deviceTag"
        @click="toggleBroadcast"
        :class="['broadcast-btn', { active: isBroadcasting, connecting: broadcastStatus === 'connecting' || broadcastStatus === 'inviting' }]"
        :title="isBroadcasting ? '点击停止广播' : '点击开始广播'"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 1a3 3 0 0 0-3 3v8a3 3 0 0 0 6 0V4a3 3 0 0 0-3-3z"/>
          <path d="M19 10v2a7 7 0 0 1-14 0v-2"/>
          <line x1="12" y1="19" x2="12" y2="23"/>
          <line x1="8" y1="23" x2="16" y2="23"/>
        </svg>
        <div v-if="isBroadcasting" class="audio-level">
          <div class="audio-level-bar" :style="{ height: audioLevel + '%' }"></div>
        </div>
      </button>
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
  },
  deviceTag: {
    type: String,
    default: ''
  },
  channelTag: {
    type: String,
    default: ''
  },
  enableBroadcast: {
    type: Boolean,
    default: false
  }
})

const emit = defineEmits(['play', 'pause', 'error', 'ended', 'stats', 'broadcastStatusChange', 'timeupdate'])

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
const playbackRate = ref(1)

const isBroadcasting = ref(false)
const broadcastStatus = ref('idle')
const broadcastError = ref('')
const audioLevel = ref(0)

let talkWs = null
let audioContext = null
let mediaStream = null
let audioProcessor = null
let audioAnalyser = null
let animationFrameId = null

let hlsInstance = null
let flvInstance = null
let wsInstance = null
let pc = null
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
let audioSessionId = 0

const currentSrc = computed(() => props.liveSrc || props.src)

  function detectProtocol(url) {
    if (!url) return 'unknown'
    if (url.includes('.m3u8')) return 'hls'
    if (url.includes('.flv')) return 'flv'
    if (url.includes('ws://') || url.includes('wss://')) return 'wsflv'
    if (url.includes('webrtc')) return 'webrtc'
    if (/\.(mp4|m4v|mov)(\?|$)/i.test(url)) return 'mp4'
    if (url.startsWith('http://') || url.startsWith('https://')) return 'http'
    return 'unknown'
  }

async function initPlayer() {
  const url = currentSrc.value
  if (!url) return

  destroyPlayer()

  loading.value = true
  error.value = null
  isVideoReady.value = false

  console.log('[VideoPlayer] initPlayer props:', { enableBroadcast: props.enableBroadcast, deviceTag: props.deviceTag })

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
      case 'webrtc':
        await initWebRTC(url)
        break
      case 'mp4':
      case 'http':
        await initNativeVideo(url)
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

  /**
   * 原生播放（MP4 / HTTP 直链）
   *
   * 录像回放的文件地址是媒体服务器上的 mp4 直链，浏览器 <video> 可直接播放，
   * 不走 hls.js / flv.js。
   */
  async function initNativeVideo(url) {
    const video = videoRef.value
    if (!video) return

    video.src = url
    video.load()

    await new Promise((resolve) => {
      let settled = false
      const done = () => {
        if (settled) return
        settled = true
        video.removeEventListener('loadedmetadata', done)
        video.removeEventListener('error', done)
        resolve()
      }
      video.addEventListener('loadedmetadata', done)
      video.addEventListener('error', done)
      setTimeout(done, 10000)
    })

    if (video.error) {
      error.value = `Video error: ${video.error.code}`
      loading.value = false
      emit('error', error.value)
      return
    }

    loading.value = false
    isVideoReady.value = true
    applyPlaybackRate()
    if (props.autoplay || props.isLive) {
      video.play().catch(() => {})
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
      applyPlaybackRate()
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
      applyPlaybackRate()
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
      applyPlaybackRate()
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
      applyPlaybackRate()
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

async function initWebRTC(url) {
  console.log('[VideoPlayer] initWebRTC called with:', url, '-', new Date().toISOString())
  loading.value = true
  error.value = null

  try {
    const urlObj = new URL(url)
    const app = urlObj.searchParams.get('app') || 'live'
    const stream = urlObj.searchParams.get('stream') || ''
    
    const signalingUrl = `http://${urlObj.host}/index/api/webrtc?app=${app}&stream=${stream}&type=play`
    console.log('[VideoPlayer] Signaling URL:', signalingUrl)

    pc = new RTCPeerConnection({
      iceServers: []
    })

    pc.addTransceiver('video', { direction: 'recvonly' })
    pc.addTransceiver('audio', { direction: 'recvonly' })

    pc.ontrack = (event) => {
      console.log('[VideoPlayer] RTCPeerConnection ontrack:', event, '-', new Date().toISOString())
      console.log('[VideoPlayer] streams[0]:', event.streams[0])
      console.log('[VideoPlayer] streams[0].getVideoTracks():', event.streams[0]?.getVideoTracks())
      console.log('[VideoPlayer] streams[0].getAudioTracks():', event.streams[0]?.getAudioTracks())
      if (videoRef.value && event.streams[0]) {
        const videoEl = videoRef.value
        videoEl.srcObject = event.streams[0]
        console.log('[VideoPlayer] video.srcObject set:', videoEl.srcObject)
        loading.value = false
        isVideoReady.value = true
        event.streams[0].getVideoTracks().forEach(track => {
          track.playoutDelayHint = 0
        })
        videoEl.play().then(() => {
          console.log('[VideoPlayer] play() succeeded -', new Date().toISOString())
        }).catch(e => {
          console.error('[VideoPlayer] play() failed:', e)
        })
      }
    }

    pc.onicecandidate = (event) => {
      console.log('[VideoPlayer] ICE candidate:', event.candidate)
    }

    pc.onconnectionstatechange = () => {
      console.log('[VideoPlayer] Connection state:', pc.connectionState, '-', new Date().toISOString())
      if (pc.connectionState === 'failed' || pc.connectionState === 'disconnected') {
        error.value = 'WebRTC connection failed'
        loading.value = false
      }
    }

    const offer = await pc.createOffer()
    await pc.setLocalDescription(offer)
    console.log('[VideoPlayer] SDP offer created, posting to:', signalingUrl)
    console.log('[VideoPlayer] SDP length:', pc.localDescription.sdp.length)

    const response = await fetch(signalingUrl, {
      method: 'POST',
      body: pc.localDescription.sdp,
      headers: { 'Content-Type': 'text/plain;charset=utf-8' }
    })

    console.log('[VideoPlayer] Response status:', response.status)
    console.log('[VideoPlayer] Response headers:', [...response.headers.entries()])
    
    if (!response.ok) {
      const text = await response.text()
      console.error('[VideoPlayer] Response error:', text)
      throw new Error(`WebRTC signaling failed: ${response.status}`)
    }

    const answerData = await response.json()
    console.log('[VideoPlayer] SDP answer received:', answerData)

    if (answerData.code !== 0) {
      throw new Error(`WebRTC signaling error: code=${answerData.code}`)
    }

    await pc.setRemoteDescription(new RTCSessionDescription({
      type: 'answer',
      sdp: answerData.sdp
    }))

    console.log('[VideoPlayer] WebRTC setup complete')
  } catch (e) {
    console.error('[VideoPlayer] WebRTC error:', e)
    error.value = `WebRTC Error: ${e.message}`
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
  if (pc) {
    pc.close()
    pc = null
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

function seek(time) {
  const video = videoRef.value
  if (!video) return
  if (isFinite(time) && time >= 0) {
    video.currentTime = time
  }
}

function applyPlaybackRate() {
  const video = videoRef.value
  if (!video) return
  const r = Number(playbackRate.value)
  if (!isFinite(r) || r <= 0) return
  try {
    video.defaultPlaybackRate = r
    video.playbackRate = r
  } catch (e) {
    console.warn('[VideoPlayer] failed to apply playbackRate:', e)
  }
}

function setPlaybackRate(rate) {
  const r = Number(rate)
  if (!isFinite(r) || r <= 0) return
  playbackRate.value = r
  applyPlaybackRate()
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

async function toggleBroadcast() {
  if (isBroadcasting.value) {
    await stopBroadcast()
  } else {
    await startBroadcast()
  }
}

async function startBroadcast() {
  if (!props.enableBroadcast || !props.deviceTag) {
    broadcastError.value = 'Broadcast not enabled'
    return
  }

  try {
    broadcastStatus.value = 'connecting'
    broadcastError.value = ''

    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsHost = window.location.host
    const channelTag = props.channelTag || props.deviceTag
    const wsUrl = `${wsProtocol}//${wsHost}/ws/audio-talk/${props.deviceTag}/${channelTag}`
    console.log('[Broadcast] Connecting to:', wsUrl)

    talkWs = new WebSocket(wsUrl)

    talkWs.onopen = () => {
      console.log('[Broadcast] WebSocket connected')
      startMicCapture()
    }

    talkWs.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        console.log('[Broadcast] Received:', data)

        if (data.status === 'ready') {
          broadcastStatus.value = 'ready'
          isBroadcasting.value = true
          if (videoRef.value) {
            videoRef.value.muted = true
          }
          emit('broadcastStatusChange', { broadcasting: true, status: 'ready' })
          startAudioLevelMonitoring()
        } else if (data.status === 'inviting') {
          broadcastStatus.value = 'inviting'
        } else if (data.status === 'error') {
          broadcastStatus.value = 'error'
          broadcastError.value = data.message || 'Unknown error'
          isBroadcasting.value = false
          if (videoRef.value) {
            videoRef.value.muted = false
          }
          emit('broadcastStatusChange', { broadcasting: false, status: 'error', error: broadcastError.value })
        } else if (data.status === 'already_started') {
          broadcastStatus.value = 'ready'
          isBroadcasting.value = true
          if (videoRef.value) {
            videoRef.value.muted = true
          }
          emit('broadcastStatusChange', { broadcasting: true, status: 'ready' })
        }
      } catch (e) {
        console.error('[Broadcast] Parse error:', e)
      }
    }

    talkWs.onerror = (e) => {
      console.error('[Broadcast] WebSocket error:', e)
      broadcastStatus.value = 'error'
      broadcastError.value = 'Connection error'
    }

    talkWs.onclose = () => {
      console.log('[Broadcast] WebSocket closed')
      if (isBroadcasting.value) {
        stopBroadcast()
      }
    }
  } catch (e) {
    console.error('[Broadcast] Start error:', e)
    broadcastStatus.value = 'error'
    broadcastError.value = e.message
  }
}

async function stopBroadcast() {
  console.log('[Broadcast] Stopping...')

  if (animationFrameId) {
    cancelAnimationFrame(animationFrameId)
    animationFrameId = null
  }

  if (mediaStream) {
    mediaStream.getTracks().forEach(track => track.stop())
    mediaStream = null
  }

  if (audioProcessor) {
    audioProcessor.disconnect()
    audioProcessor = null
  }

  if (audioContext) {
    await audioContext.close()
    audioContext = null
  }

  if (talkWs) {
    if (talkWs.readyState === WebSocket.OPEN) {
      talkWs.send(JSON.stringify({ action: 'stop_talk' }))
    }
    talkWs.close()
    talkWs = null
  }

  isBroadcasting.value = false
  broadcastStatus.value = 'idle'
  audioLevel.value = 0

  if (videoRef.value) {
    videoRef.value.muted = false
  }

  emit('broadcastStatusChange', { broadcasting: false, status: 'idle' })
}

async function startMicCapture() {
  audioSessionId++
  const currentSessionId = audioSessionId

  try {
    mediaStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true
      }
    })

    audioContext = new (window.AudioContext || window.webkitAudioContext)()
    const source = audioContext.createMediaStreamSource(mediaStream)

    const micGain = audioContext.createGain()
    micGain.gain.value = 0.3

    audioAnalyser = audioContext.createAnalyser()
    audioAnalyser.fftSize = 256
    source.connect(micGain)
    micGain.connect(audioAnalyser)

    const bufferSize = 2048
    audioProcessor = audioContext.createScriptProcessor(bufferSize, 1, 1)

    audioProcessor.onaudioprocess = (e) => {
      if (currentSessionId !== audioSessionId) {
        return
      }
      if (!isBroadcasting.value || !talkWs || talkWs.readyState !== WebSocket.OPEN) {
        return
      }

      const inputData = e.inputBuffer.getChannelData(0)
      const pcmData = new Int16Array(inputData.length)

      for (let i = 0; i < inputData.length; i++) {
        const s = Math.max(-1, Math.min(1, inputData[i]))
        pcmData[i] = s < 0 ? s * 0x8000 : s * 0x7FFF
      }

      const buffer = new ArrayBuffer(pcmData.length * 2)
      const view = new DataView(buffer)
      for (let i = 0; i < pcmData.length; i++) {
        view.setInt16(i * 2, pcmData[i], true)
      }

      if (talkWs && talkWs.readyState === WebSocket.OPEN) {
        talkWs.send(buffer)
      }
    }

    const dest = audioContext.createMediaStreamDestination()
    micGain.connect(audioProcessor)
    audioProcessor.connect(dest)

    console.log('[Broadcast] Sending start_talk action');
    talkWs.send(JSON.stringify({ action: 'start_talk' }))
    broadcastStatus.value = 'inviting'
  } catch (e) {
    console.error('[Broadcast] Mic capture error:', e)
    broadcastStatus.value = 'error'
    broadcastError.value = `Microphone error: ${e.message}`
    isBroadcasting.value = false
  }
}

function startAudioLevelMonitoring() {
  if (!audioAnalyser) return

  const dataArray = new Uint8Array(audioAnalyser.frequencyBinCount)

  function update() {
    if (!isBroadcasting.value || !audioAnalyser) return

    audioAnalyser.getByteFrequencyData(dataArray)
    const average = dataArray.reduce((a, b) => a + b, 0) / dataArray.length
    audioLevel.value = Math.min(100, Math.round((average / 128) * 100))

    animationFrameId = requestAnimationFrame(update)
  }

  update()
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
      emit('timeupdate', { currentTime: video.currentTime, duration: video.duration })
    })

    video.addEventListener('durationchange', () => {
      duration.value = video.duration
    })

    video.addEventListener('waiting', () => {
      console.log('[VideoPlayer] video waiting (buffering)')
      loading.value = true
    })

    video.addEventListener('playing', () => {
      console.log('[VideoPlayer] video playing event - latency:', video.currentTime, '-', new Date().toISOString())
      loading.value = false
    })

    video.addEventListener('canplay', () => {
      console.log('[VideoPlayer] video canplay event')
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
  if (isBroadcasting.value) {
    stopBroadcast()
  }
  destroyPlayer()
})

defineExpose({
  play: togglePlay,
  pause: togglePlay,
  reconnect,
  destroy: destroyPlayer,
  startBroadcast,
  stopBroadcast,
  seek,
  setPlaybackRate
})
</script>

<style scoped>
.video-player {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 160px;
  background: #000;
  overflow: hidden;
}

.empty-state-wrapper {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: #999;
}

.empty-state-wrapper svg {
  width: 48px;
  height: 48px;
  margin: 0 auto 8px;
}

.video-container {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  background: #000;
}

.video-container video {
  width: 100%;
  height: 100%;
  max-width: 100%;
  max-height: 100%;
  display: block;
  object-fit: contain;
  background: #000;
}

.controls {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  padding: 24px 12px 10px;
  box-sizing: border-box;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  background: linear-gradient(to top, rgba(0, 0, 0, 0.85) 0%, rgba(0, 0, 0, 0.4) 60%, transparent 100%);
  z-index: 10;
  transition: opacity 0.25s ease;
}

.controls.hidden {
  opacity: 0;
  pointer-events: none;
}

.controls-left,
.controls-right {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.controls-right {
  justify-content: flex-end;
  flex-wrap: nowrap;
}

.controls button {
  flex-shrink: 0;
  width: 28px;
  height: 28px;
  background: rgba(255, 255, 255, 0.18);
  border: none;
  border-radius: 4px;
  padding: 0;
  cursor: pointer;
  color: white;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s;
}

.controls button:hover {
  background: rgba(255, 255, 255, 0.35);
}

.controls button svg {
  width: 16px;
  height: 16px;
}

.time-display,
.bitrate-display {
  color: #fff;
  font-size: 12px;
  font-family: Arial, sans-serif;
  white-space: nowrap;
}

.live-tag {
  background: #e74c3c;
  color: #fff;
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 10px;
  font-weight: bold;
  line-height: 1.4;
}

.overlay {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 5;
  pointer-events: none;
}

.loading-overlay {
  background: rgba(0, 0, 0, 0.4);
  pointer-events: auto;
}

.error-overlay {
  background: rgba(0, 0, 0, 0.7);
  pointer-events: auto;
}

.error-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: #fff;
  font-size: 13px;
}

.error-inner svg {
  width: 32px;
  height: 32px;
  color: #ef4444;
}

.error-inner button {
  margin-top: 4px;
  padding: 4px 12px;
  border-radius: 4px;
  border: none;
  background: #3b82f6;
  color: #fff;
  cursor: pointer;
}

.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid rgba(255, 255, 255, 0.25);
  border-top-color: #fff;
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.broadcast-btn {
  position: absolute;
  right: 10px;
  bottom: 52px;
  background: rgba(59, 130, 246, 0.85);
  border: none;
  color: #fff;
  z-index: 100;
  padding: 8px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.15s;
}

.broadcast-btn:hover {
  background: rgba(59, 130, 246, 1);
}

.broadcast-btn.active {
  background: rgba(239, 68, 68, 0.9);
  animation: pulse-broadcast 1.5s ease-in-out infinite;
}

.broadcast-btn.connecting {
  background: rgba(245, 158, 11, 0.9);
  animation: pulse-connecting 1s ease-in-out infinite;
}

.broadcast-btn svg {
  width: 18px;
  height: 18px;
}

.broadcast-btn.active svg {
  stroke: white;
}

.audio-level {
  position: absolute;
  bottom: -2px;
  left: 50%;
  transform: translateX(-50%);
  width: 4px;
  height: 16px;
  background: rgba(0, 0, 0, 0.5);
  border-radius: 2px;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  overflow: hidden;
}

.audio-level-bar {
  width: 100%;
  background: linear-gradient(to top, #22c55e, #eab308, #ef4444);
  border-radius: 2px;
  transition: height 0.05s ease-out;
}

.broadcast-status {
  display: flex;
  align-items: center;
  gap: 4px;
}

.status-text {
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 3px;
}

.status-text.connecting {
  background: rgba(245, 158, 11, 0.8);
  color: white;
}

.status-text.inviting {
  background: rgba(245, 158, 11, 0.8);
  color: white;
}

.status-text.ready {
  background: rgba(239, 68, 68, 0.8);
  color: white;
  animation: blink 1s ease-in-out infinite;
}

.status-text.error {
  background: rgba(239, 68, 68, 0.8);
  color: white;
}

@keyframes pulse-broadcast {
  0%, 100% {
    box-shadow: 0 0 0 0 rgba(239, 68, 68, 0.4);
  }
  50% {
    box-shadow: 0 0 0 6px rgba(239, 68, 68, 0);
  }
}

@keyframes pulse-connecting {
  0%, 100% {
    box-shadow: 0 0 0 0 rgba(245, 158, 11, 0.4);
  }
  50% {
    box-shadow: 0 0 0 4px rgba(245, 158, 11, 0);
  }
}

@keyframes blink {
  0%, 100% {
    opacity: 1;
  }
  50% {
    opacity: 0.6;
  }
}
</style>


