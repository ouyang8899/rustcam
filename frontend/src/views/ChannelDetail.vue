<template>
  <div class="page-container">
    <div class="page-header">
      <div class="header-left">
        <el-button text @click="$router.back()">
          <el-icon><ArrowLeft /></el-icon> 返回
        </el-button>
        <h1 class="page-title">{{ channel?.name || '通道详情' }}</h1>
      </div>
      <div class="page-toolbar">
        <el-button :icon="Refresh" @click="fetchData">刷新</el-button>
      </div>
    </div>

    <div class="page-body" v-if="channel">
      <div class="detail-layout">
        <div class="detail-main">
          <el-card shadow="never" class="player-card">
            <template #header>
              <div class="card-header">
                <span>视频预览</span>
                <div class="stream-selector">
                  <el-radio-group v-model="selectedStreamType" size="small">
                    <el-radio-button value="hls" :disabled="!playLinks?.hls">HLS</el-radio-button>
                    <el-radio-button value="flv" :disabled="!playLinks?.flv">FLV</el-radio-button>
                    <el-radio-button value="wsflv" :disabled="!playLinks?.web_flv">WS-FLV</el-radio-button>
                    <el-radio-button value="rtsp" :disabled="!playLinks?.rtsp_signaling">RTSP</el-radio-button>
                  </el-radio-group>
                </div>
              </div>
            </template>
            <div class="player-with-ptz">
              <div class="player-wrapper">
                <VideoPlayer
                  v-if="currentStreamUrl && !loading"
                  :liveSrc="currentStreamUrl"
                  :type="selectedStreamType"
                  :isLive="true"
                  :autoplay="true"
                  :deviceTag="route.params.deviceTag"
                  :channelTag="route.params.channelTag"
                  :enableBroadcast="true"
                />
                <div v-else-if="loading" class="loading-state">
                  <el-icon class="is-loading"><Loading /></el-icon>
                  <span>加载中...</span>
                </div>
                <div v-else class="empty-state">
                  <el-icon><VideoPlay /></el-icon>
                  <span>暂无视频流</span>
                </div>
              </div>

              <div v-if="device && (device.protocol?.toLowerCase() === 'onvif' || device.protocol?.toLowerCase() === 'gb28181')" class="ptz-sidebar">
                <div class="ptz-control-header">
                  <span class="ptz-label">云台控制</span>
                  <div class="ptz-status-dot" :class="{ active: ptzStatus?.moving }"></div>
                </div>
                <div class="ptz-dpad">
                  <button class="dpad-btn dpad-up" @mousedown="ptzControl('up', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 4l-8 8h5v8h6v-8h5z"/></svg>
                  </button>
                  <button class="dpad-btn dpad-left" @mousedown="ptzControl('left', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M4 12l8-8v5h8v6h-8V4z"/></svg>
                  </button>
                  <button class="dpad-center" @click="ptzControl('stop', 1)">
                    <svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>
                  </button>
                  <button class="dpad-btn dpad-right" @mousedown="ptzControl('right', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M20 12l-8 8v-5H4v-6h8V4z"/></svg>
                  </button>
                  <button class="dpad-btn dpad-down" @mousedown="ptzControl('down', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 20l8-8h-5V4h-6v8H4z"/></svg>
                  </button>
                </div>
                <div class="ptz-aux-row">
                  <div class="aux-group">
                    <span class="aux-label">变倍</span>
                    <div class="aux-btns">
                      <button class="aux-btn" @mousedown="ptzControl('zoom_in', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">+</button>
                      <button class="aux-btn" @mousedown="ptzControl('zoom_out', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">−</button>
                    </div>
                  </div>
                  <div class="aux-group">
                    <span class="aux-label">聚焦</span>
                    <div class="aux-btns">
                      <button class="aux-btn" @mousedown="ptzControl('focus_in', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">+</button>
                      <button class="aux-btn" @mousedown="ptzControl('focus_out', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">−</button>
                    </div>
                  </div>
                  <div class="aux-group">
                    <span class="aux-label">光圈</span>
                    <div class="aux-btns">
                      <button class="aux-btn" @mousedown="ptzControl('iris_in', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">+</button>
                      <button class="aux-btn" @mousedown="ptzControl('iris_out', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">−</button>
                    </div>
                  </div>
                </div>
                <div class="ptz-joystick-area" ref="joystickRef" @mousedown="startJoystick" @touchstart.prevent="startJoystick">
                  <div class="joystick-base">
                    <div class="joystick-stick" :style="joystickStyle"></div>
                  </div>
                </div>
                <div class="ptz-position" v-if="ptzStatus?.position">
                  <span class="pos-label">位置</span>
                  <span class="pos-value">P:{{ ptzStatus.position.pan?.toFixed(2) }} T:{{ ptzStatus.position.tilt?.toFixed(2) }} Z:{{ ptzStatus.position.zoom?.toFixed(2) }}</span>
                </div>
              </div>
            </div>
          </el-card>

          <el-card v-if="device && (device.protocol?.toLowerCase() === 'onvif' || device.protocol?.toLowerCase() === 'gb28181')" shadow="never" class="preset-card">
            <template #header>
              <div class="preset-header">
                <span>预制位</span>
                <el-button size="small" type="primary" @click="showPresetDialog = true">保存当前</el-button>
              </div>
            </template>
            <div v-if="presets.length === 0" class="empty-presets">暂无预制位</div>
            <div v-else class="preset-grid">
              <div v-for="preset in presets" :key="preset.token" class="preset-tile">
                <div class="preset-info">
                  <span class="preset-name">{{ preset.name }}</span>
                </div>
                <div class="preset-actions">
                  <el-button size="small" type="primary" @click="gotoPreset(preset)">调用</el-button>
                  <el-button size="small" type="danger" plain @click="removePreset(preset)">删除</el-button>
                </div>
              </div>
            </div>
          </el-card>

          <el-card shadow="never" class="links-card">
            <template #header><span>播放链接</span></template>
            <div class="links-list" v-if="playLinks">
              <div class="link-item" v-if="playLinks.rtsp_signaling">
                <div class="link-info">
                  <el-tag size="small" type="info">RTSP</el-tag>
                  <span class="link-url">{{ playLinks.rtsp_signaling }}</span>
                </div>
                <el-button size="small" @click="copyLink(playLinks.rtsp_signaling)">复制</el-button>
              </div>
              <div class="link-item" v-if="playLinks.flv">
                <div class="link-info">
                  <el-tag size="small" type="warning">FLV</el-tag>
                  <span class="link-url">{{ playLinks.flv }}</span>
                </div>
                <el-button size="small" @click="copyLink(playLinks.flv)">复制</el-button>
              </div>
              <div class="link-item" v-if="playLinks.web_flv">
                <div class="link-info">
                  <el-tag size="small" type="danger">HTTP-FLV</el-tag>
                  <span class="link-url">{{ playLinks.web_flv }}</span>
                </div>
                <el-button size="small" @click="copyLink(playLinks.web_flv)">复制</el-button>
              </div>
              <div class="link-item" v-if="playLinks.hls">
                <div class="link-info">
                  <el-tag size="small" type="success">HLS</el-tag>
                  <span class="link-url">{{ playLinks.hls }}</span>
                </div>
                <el-button size="small" @click="copyLink(playLinks.hls)">复制</el-button>
              </div>
              <div class="link-item" v-if="playLinks.webrtc">
                <div class="link-info">
                  <el-tag size="small" type="primary">WebRTC</el-tag>
                  <span class="link-url">{{ playLinks.webrtc }}</span>
                </div>
                <el-button size="small" @click="copyLink(playLinks.webrtc)">复制</el-button>
              </div>
            </div>
            <el-empty v-else description="暂无播放链接" />
          </el-card>
        </div>

        <div class="detail-sidebar">
          <el-card shadow="never">
            <template #header><span>通道信息</span></template>
            <el-descriptions :column="1" border size="small">
              <el-descriptions-item label="设备标识">{{ channel.device_tag }}</el-descriptions-item>
              <el-descriptions-item label="通道标识">{{ channel.channel_tag }}</el-descriptions-item>
              <el-descriptions-item label="状态">
                <el-tag size="small" :type="statusType">{{ channel.status }}</el-tag>
              </el-descriptions-item>
              <el-descriptions-item label="类型">{{ channel.device_type }}</el-descriptions-item>
              <el-descriptions-item label="地址">{{ channel.ip_address ? (channel.port ? `${channel.ip_address}:${channel.port}` : channel.ip_address) : '-' }}</el-descriptions-item>
              <el-descriptions-item label="厂商">{{ channel.manufacturer || '-' }}</el-descriptions-item>
              <el-descriptions-item label="型号">{{ channel.model || '-' }}</el-descriptions-item>
              <el-descriptions-item label="创建时间">{{ formatDate(channel.created_at) }}</el-descriptions-item>
            </el-descriptions>
          </el-card>
        </div>
      </div>
    </div>

    <el-dialog v-model="showPresetDialog" title="保存预制位" width="400px">
      <el-input v-model="newPresetName" placeholder="例如: 大门入口" />
      <template #footer>
        <el-button @click="showPresetDialog = false">取消</el-button>
        <el-button type="primary" :disabled="!newPresetName.trim()" @click="savePreset">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ArrowLeft, Refresh, VideoPlay, Loading } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'
import { useChannelStore } from '../stores/channelStore'
import { useDeviceStore } from '../stores/deviceStore'
import VideoPlayer from '../components/VideoPlayer.vue'

const route = useRoute()
const router = useRouter()
const channelStore = useChannelStore()
const deviceStore = useDeviceStore()

const channel = ref(null)
const device = ref(null)
const playLinks = ref(null)
const loading = ref(false)
const selectedStreamType = ref('hls')
const ptzSpeed = ref(50)
const presets = ref([])
const showPresetDialog = ref(false)
const newPresetName = ref('')
let ptzStopTimer = null
const joystickRef = ref(null)
const joystickActive = ref(false)
const joystickDelta = ref({ x: 0, y: 0 })
const ptzStatus = ref(null)
const ptzStatusTimer = ref(null)

const joystickStyle = computed(() => ({
  transform: `translate(${joystickDelta.value.x * 30}px, ${joystickDelta.value.y * 30}px)`
}))

const statusType = computed(() => {
  if (!channel.value) return 'info'
  const s = channel.value.status?.toLowerCase()
  if (s === 'online') return 'success'
  if (s === 'maintaining') return 'warning'
  if (s === 'error') return 'danger'
  return 'info'
})

const isGb28181 = computed(() => device.value?.protocol?.toLowerCase() === 'gb28181')
const isOnvif = computed(() => device.value?.protocol?.toLowerCase() === 'onvif')

const currentStreamUrl = computed(() => {
  if (!playLinks.value) return null
  switch (selectedStreamType.value) {
    case 'hls': return playLinks.value.hls
    case 'flv': return playLinks.value.flv
    case 'wsflv': return playLinks.value.web_flv
    case 'rtsp': return playLinks.value.rtsp_signaling
    default: return playLinks.value.hls || playLinks.value.web_flv || null
  }
})

async function fetchData() {
  loading.value = true
  try {
    const { deviceTag, channelTag } = route.params
    channel.value = await channelStore.getChannel(deviceTag, channelTag)
    playLinks.value = await channelStore.getChannelPlayLinks(deviceTag, channelTag)
    
    const dev = await deviceStore.fetchDevice(deviceTag)
    device.value = dev || null
    
    if (device.value) {
      const proto = (device.value.protocol || '').toLowerCase()
      if (proto === 'onvif' || proto === 'gb28181') {
        await fetchPresets()
      }
    }
    updateDefaultStreamType()
  } catch (e) {
    ElMessage.error('获取通道信息失败')
  } finally {
    loading.value = false
  }
}

function updateDefaultStreamType() {
  if (playLinks.value?.hls) selectedStreamType.value = 'hls'
  else if (playLinks.value?.web_flv) selectedStreamType.value = 'wsflv'
  else if (playLinks.value?.flv) selectedStreamType.value = 'flv'
  else if (playLinks.value?.rtsp_signaling) selectedStreamType.value = 'rtsp'
}

async function fetchPresets() {
  try {
    const { deviceTag, channelTag } = route.params
    presets.value = await channelStore.getChannelPtzPresets(deviceTag, channelTag)
  } catch {
    presets.value = []
  }
}

async function ptzControl(cmd) {
  clearTimeout(ptzStopTimer)
  try {
    const { deviceTag, channelTag } = route.params
    await channelStore.channelPtzControl(deviceTag, channelTag, cmd, ptzSpeed.value)
  } catch (e) {
    console.error('PTZ control failed:', e)
  }
}

function ptzStop() {
  ptzStopTimer = setTimeout(async () => {
    try {
      const { deviceTag, channelTag } = route.params
      await channelStore.channelPtzControl(deviceTag, channelTag, 'stop', ptzSpeed.value)
    } catch {}
  }, 100)
}

async function savePreset() {
  if (!newPresetName.value.trim()) return
  try {
    const { deviceTag, channelTag } = route.params
    await channelStore.createChannelPtzPreset(deviceTag, channelTag, newPresetName.value.trim())
    await fetchPresets()
    showPresetDialog.value = false
    newPresetName.value = ''
    ElMessage.success('预制位已保存')
  } catch {
    ElMessage.error('保存预制位失败')
  }
}

async function gotoPreset(preset) {
  try {
    const { deviceTag, channelTag } = route.params
    await channelStore.channelPtzControl(deviceTag, channelTag, 'goto_preset', ptzSpeed.value, preset.token)
  } catch {
    ElMessage.error('调用预制位失败')
  }
}

async function removePreset(preset) {
  try {
    const { deviceTag, channelTag } = route.params
    await channelStore.deleteChannelPtzPreset(deviceTag, channelTag, preset.token)
    await fetchPresets()
    ElMessage.success('预制位已删除')
  } catch {
    ElMessage.error('删除预制位失败')
  }
}

function startJoystick(e) {
  joystickActive.value = true
  updateJoystick(e)
}

function updateJoystick(e) {
  if (!joystickActive.value || !joystickRef.value) return
  
  const rect = joystickRef.value.getBoundingClientRect()
  const centerX = rect.left + rect.width / 2
  const centerY = rect.top + rect.height / 2
  
  const clientX = e.touches ? e.touches[0].clientX : e.clientX
  const clientY = e.touches ? e.touches[0].clientY : e.clientY
  
  let dx = (clientX - centerX) / (rect.width / 2)
  let dy = (clientY - centerY) / (rect.height / 2)
  
  const dist = Math.sqrt(dx * dx + dy * dy)
  if (dist > 1) {
    dx /= dist
    dy /= dist
  }
  
  joystickDelta.value = { x: dx, y: dy }
  
  if (Math.abs(dx) > 0.1 || Math.abs(dy) > 0.1) {
    const { deviceTag, channelTag } = route.params
    channelStore.channelPtzControl(deviceTag, channelTag, {
      type: 'continuous',
      pan: dx,
      tilt: -dy,
      zoom: 0
    }, ptzSpeed.value).catch(() => {})
  }
}

function stopJoystick() {
  if (!joystickActive.value) return
  joystickActive.value = false
  joystickDelta.value = { x: 0, y: 0 }
  
  const { deviceTag, channelTag } = route.params
  channelStore.channelPtzControl(deviceTag, channelTag, 'stop', ptzSpeed.value)
}

async function fetchPtzStatus() {
  if (!channel.value?.device_tag || !channel.value?.channel_tag) return
  try {
    const { deviceTag, channelTag } = route.params
    const status = await channelStore.getChannelPtzStatus(deviceTag, channelTag)
    ptzStatus.value = status
  } catch {
    ptzStatus.value = null
  }
}

function startPtzStatusPolling() {
  fetchPtzStatus()
  ptzStatusTimer.value = setInterval(fetchPtzStatus, 5000)
}

function stopPtzStatusPolling() {
  if (ptzStatusTimer.value) {
    clearInterval(ptzStatusTimer.value)
    ptzStatusTimer.value = null
  }
}

function copyLink(url) {
  navigator.clipboard.writeText(url)
  ElMessage.success('已复制到剪贴板')
}

function formatDate(dateStr) {
  if (!dateStr) return '-'
  return new Date(dateStr).toLocaleString('zh-CN')
}

onMounted(() => {
  fetchData()
  
  document.addEventListener('mousemove', updateJoystick)
  document.addEventListener('mouseup', stopJoystick)
  document.addEventListener('touchmove', updateJoystick)
  document.addEventListener('touchend', stopJoystick)
  
  const proto = device.value?.protocol?.toLowerCase()
  if (proto === 'onvif' || proto === 'gb28181') {
    startPtzStatusPolling()
  }
})

onUnmounted(() => {
  document.removeEventListener('mousemove', updateJoystick)
  document.removeEventListener('mouseup', stopJoystick)
  document.removeEventListener('touchmove', updateJoystick)
  document.removeEventListener('touchend', stopJoystick)
  stopPtzStatusPolling()
})
</script>

<style scoped>
.page-container {
  padding: 20px;
  min-height: 100vh;
  background: var(--el-bg-page);
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.header-left {
  display: flex;
  align-items: center;
  gap: 16px;
}

.page-title {
  font-size: 20px;
  font-weight: 600;
  margin: 0;
}

.page-toolbar {
  display: flex;
  gap: 8px;
}

.detail-layout {
  display: grid;
  grid-template-columns: 1fr 360px;
  gap: 16px;
}

.detail-main {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.detail-sidebar {
  display: flex;
  flex-direction: column;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.player-card :deep(.el-card__header) {
  padding: 12px 16px;
}

.player-wrapper {
  background: #000;
  border-radius: 4px;
  overflow: hidden;
  position: relative;
  min-width: 0;
}

.loading-state,
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  color: #666;
  font-size: 14px;
}

.empty-state .el-icon {
  font-size: 48px;
  color: #999;
}

.player-with-ptz {
  display: flex;
  gap: 16px;
  align-items: stretch;
}

.player-wrapper {
  flex: 1;
  aspect-ratio: 16 / 9;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 0;
}

.ptz-sidebar {
  width: 160px;
  flex-shrink: 0;
  background: linear-gradient(180deg, var(--el-fill-color-lighter) 0%, var(--el-fill-color-dark) 100%);
  border-radius: var(--radius-base);
  padding: 14px 10px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  border: 1px solid var(--el-border-color-lighter);
}

.ptz-control-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.ptz-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.ptz-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--el-text-color-dim);
  transition: all 0.3s;
}

.ptz-status-dot.active {
  background: var(--el-color-success);
  box-shadow: 0 0 8px var(--el-color-success);
}

.ptz-dpad {
  display: grid;
  grid-template-columns: repeat(3, 40px);
  grid-template-rows: repeat(3, 40px);
  gap: 4px;
  justify-content: center;
}

.dpad-btn {
  width: 40px;
  height: 40px;
  border: 1px solid var(--el-border-color);
  background: var(--el-fill-color-light);
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s;
  color: var(--el-text-color-primary);
}

.dpad-btn svg {
  width: 18px;
  height: 18px;
}

.dpad-btn:hover {
  background: var(--el-color-primary-light-9);
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
}

.dpad-btn:active {
  transform: scale(0.92);
  background: var(--el-color-primary);
  color: white;
}

.dpad-center {
  width: 40px;
  height: 40px;
  border: 1px solid rgba(239, 68, 68, 0.3);
  background: rgba(239, 68, 68, 0.06);
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s;
  color: var(--el-color-danger);
}

.dpad-center svg {
  width: 16px;
  height: 16px;
}

.dpad-center:hover {
  background: rgba(239, 68, 68, 0.15);
  border-color: var(--el-color-danger);
}

.dpad-center:active {
  transform: scale(0.92);
  background: var(--el-color-danger);
  color: white;
}

.dpad-up { grid-column: 2; grid-row: 1; }
.dpad-left { grid-column: 1; grid-row: 2; }
.dpad-center { grid-column: 2; grid-row: 2; }
.dpad-right { grid-column: 3; grid-row: 2; }
.dpad-down { grid-column: 2; grid-row: 3; }

.ptz-aux-row {
  display: flex;
  gap: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.aux-group {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.aux-label {
  font-size: 9px;
  color: var(--el-text-color-secondary);
  text-align: center;
  text-transform: uppercase;
  letter-spacing: 0.3px;
}

.aux-btns {
  display: flex;
  gap: 2px;
}

.aux-btn {
  flex: 1;
  height: 26px;
  border: 1px solid var(--el-border-color);
  background: var(--el-fill-color-light);
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  font-weight: 500;
  color: var(--el-text-color-primary);
  transition: all 0.15s;
}

.aux-btn:hover {
  border-color: var(--el-color-primary);
  color: var(--el-color-primary);
}

.aux-btn:active {
  background: var(--el-color-primary);
  color: white;
}

.ptz-joystick-area {
  display: flex;
  justify-content: center;
  padding: 8px 0;
  border-top: 1px solid var(--el-border-color-lighter);
}

.ptz-joystick-area .joystick-base {
  width: 80px;
  height: 80px;
  border-radius: 50%;
  background: var(--el-fill-color-light);
  border: 2px solid var(--el-border-color);
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
}

.ptz-joystick-area .joystick-stick {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--el-color-primary-light-3) 0%, var(--el-color-primary) 100%);
  box-shadow: 0 2px 8px rgba(0,0,0,0.3);
  transition: transform 0.05s ease-out;
  pointer-events: none;
}

.ptz-position {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  background: var(--el-fill-color-dark);
  border-radius: 6px;
  border: 1px solid var(--el-border-color-lighter);
}

.pos-label {
  font-size: 9px;
  color: var(--el-text-color-dim);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.pos-value {
  font-size: 10px;
  font-family: var(--font-mono);
  color: var(--el-text-color-secondary);
}

.preset-card :deep(.el-card__header) {
  padding: 12px 16px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.preset-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.preset-header span {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.preset-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 10px;
}

.preset-tile {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  transition: all 0.2s;
}

.preset-tile:hover {
  border-color: var(--el-color-primary);
  box-shadow: 0 2px 12px rgba(0,0,0,0.08);
}

.preset-info {
  display: flex;
  align-items: center;
  gap: 6px;
}

.preset-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-primary);
}

.preset-actions {
  display: flex;
  gap: 4px;
}

.preset-actions .el-button {
  flex: 1;
  padding: 4px 8px;
  font-size: 11px;
}

.empty-presets {
  text-align: center;
  padding: 32px 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.links-card .links-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.link-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  background: var(--el-fill-color-lighter);
  border-radius: 6px;
  gap: 12px;
}

.link-info {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  min-width: 0;
}

.link-url {
  font-family: 'SF Mono', Monaco, monospace;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.stream-selector {
  display: flex;
  gap: 4px;
}

.stream-selector :deep(.el-radio-button__inner) {
  padding: 5px 12px;
  font-size: 12px;
}

@media (max-width: 1024px) {
  .detail-layout {
    grid-template-columns: 1fr;
  }
}
</style>
