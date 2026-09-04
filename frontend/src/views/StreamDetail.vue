<template>
  <div class="page-root">
    <div class="body-content">
      <div class="page-header">
        <div class="header-left">
          <el-button text @click="$router.back()">
            <el-icon><Back /></el-icon> 返回
          </el-button>
          <el-divider direction="vertical" />
          <span class="page-title">{{ stream?.stream_key || `流 #${id}` }}</span>
          <el-tag v-if="stream" :type="stateType(stream.state)" size="small" style="margin-left: 8px">
            {{ stateLabel(stream.state) }}
          </el-tag>
        </div>
        <div class="header-right">
          <el-button :icon="Refresh" size="small" :loading="loading" @click="fetchData">刷新</el-button>
          <el-button 
            v-if="stream?.state === 'Error' || stream?.state === 'Recovering' || stream?.state === 'Idle' || stream?.state === 'Stopped'" 
            type="success" 
            plain 
            size="small" 
            :loading="starting" 
            @click="confirmRestart"
          >重启</el-button>
          <el-button 
            v-if="stream?.state === 'Active' || stream?.state === 'Starting'" 
            type="danger" 
            plain 
            size="small" 
            :loading="stopping" 
            @click="confirmStop"
          >停止流</el-button>
        </div>
      </div>

      <el-row :gutter="16" v-if="stream">
        <el-col :span="16">
          <el-card shadow="never">
            <template #header>
              <span>直播预览</span>
            </template>
            <div class="player-with-ptz">
              <div class="player-wrapper">
                <VideoPlayer
                  v-if="playingUrl"
                  ref="playerRef"
                  :liveSrc="playingUrl"
                  :type="playingType"
                  :isLive="true"
                  :autoplay="true"
                />
                <div v-else class="player-placeholder">
                  <el-icon size="48"><VideoPlay /></el-icon>
                  <p>暂无直播流</p>
                  <el-button type="primary" size="small" @click="startPreview" :loading="previewLoading">
                    开始预览
                  </el-button>
                </div>
              </div>

              <div v-if="supportPtz && stream?.channel_tag" class="ptz-sidebar">
                <div class="ptz-control-header">
                  <span class="ptz-label">云台控制</span>
                  <div class="ptz-status-dot" :class="{ active: ptzStatus?.moving }"></div>
                </div>
                <div class="ptz-dpad">
                  <button class="dpad-btn dpad-up" @mousedown="ptzControl('up', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 4l-8 8h5v8h6v-8h5z"/></svg>
                  </button>
                  <button class="dpad-btn dpad-left" @mousedown="ptzControl('left', 1)" @mouseup="ptzStop" @mouseleave="ptzStop">
                    <svg viewBox="0 0 24 24" fill="currentColor"><path d="M4 12l8-8v5h8v6h-8v5z"/></svg>
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

          <el-card v-if="supportPtz && stream?.channel_tag" shadow="never" class="preset-card">
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

          <el-card shadow="never" style="margin-top: 16px">
            <template #header>
              <span>直播链接</span>
              <el-button text size="small" style="float: right" @click="copyAll">
                复制全部
              </el-button>
            </template>
            <el-table :data="playLinkRows" border size="small">
              <el-table-column label="协议" width="100" align="center">
                <template #default="{ row }">
                  <el-tag size="small">{{ row.protocol }}</el-tag>
                </template>
              </el-table-column>
              <el-table-column label="地址" min-width="300">
                <template #default="{ row }">
                  <el-tooltip :content="row.url" placement="top" :show-after="300">
                    <span class="url-text">{{ row.url }}</span>
                  </el-tooltip>
                </template>
              </el-table-column>
              <el-table-column label="操作" width="120" align="center">
                <template #default="{ row }">
                  <el-button size="small" @click="playProtocol(row.protocol, row.url)">播放</el-button>
                  <el-button size="small" @click="copyUrl(row.url)">复制</el-button>
                </template>
              </el-table-column>
            </el-table>
          </el-card>
        </el-col>

        <el-col :span="8">
          <el-card shadow="never">
            <template #header>基本信息</template>
            <el-descriptions :column="1" border size="small">
              <el-descriptions-item label="流 ID">{{ stream.id }}</el-descriptions-item>
              <el-descriptions-item label="流标识">{{ stream.stream_key }}</el-descriptions-item>
              <el-descriptions-item label="状态">
                <el-tag :type="stateType(stream.state)" size="small">{{ stateLabel(stream.state) }}</el-tag>
              </el-descriptions-item>
              <el-descriptions-item label="自动恢复">
                <el-tag size="small" :type="stream.auto_recover ? 'success' : 'info'">
                  {{ stream.auto_recover ? '是' : '否' }}
                </el-tag>
              </el-descriptions-item>
              <el-descriptions-item label="重试次数">{{ stream.retry_count || 0 }} / {{ stream.max_retries || 20 }}</el-descriptions-item>
              <el-descriptions-item label="错误信息" v-if="stream.last_error">
                <span style="color: var(--color-danger); font-size: 12px;">{{ stream.last_error }}</span>
              </el-descriptions-item>
              <el-descriptions-item label="观看人数">{{ stream.viewer_count || 0 }}</el-descriptions-item>
              <el-descriptions-item label="媒体服务器">{{ stream.media_server_tag }}</el-descriptions-item>
              <el-descriptions-item label="应用">{{ stream.app }}</el-descriptions-item>
              <el-descriptions-item label="设备标识">{{ stream.device_tag || '-' }}</el-descriptions-item>
              <el-descriptions-item label="通道标识">{{ stream.channel_tag || '-' }}</el-descriptions-item>
              <el-descriptions-item label="创建时间">{{ formatTime(stream.created_at) }}</el-descriptions-item>
            </el-descriptions>
          </el-card>

          <el-card shadow="never" style="margin-top: 16px">
            <template #header>
              <span>设备信息</span>
              <el-button text size="small" style="float: right" @click="$router.push(`/devices/${device.device_tag}`)">
                查看详情
              </el-button>
            </template>
            <div v-if="device">
              <el-descriptions :column="1" border size="small">
                <el-descriptions-item label="名称">{{ device.name }}</el-descriptions-item>
                <el-descriptions-item label="协议">
                  <el-tag size="small">{{ device.protocol }}</el-tag>
                </el-descriptions-item>
                <el-descriptions-item label="状态">
                  <span class="status-dot" :class="device.status?.toLowerCase()" />
                  {{ device.status }}
                </el-descriptions-item>
                <el-descriptions-item label="设备地址">{{ device.host }}:{{ device.port }}</el-descriptions-item>
                <el-descriptions-item label="媒体服务器">{{ device.media_server_tag || '-' }}</el-descriptions-item>
                <el-descriptions-item label="应用">{{ device.app || '-' }}</el-descriptions-item>
                <el-descriptions-item label="流标识">{{ device.stream_key || '-' }}</el-descriptions-item>
                <el-descriptions-item label="设备标签">{{ device.device_tag || '-' }}</el-descriptions-item>
                <el-descriptions-item label="父设备">{{ device.parent_device_tag || '-' }}</el-descriptions-item>
                <el-descriptions-item label="公开流">
                  <el-tag size="small" :type="device.is_public ? 'success' : 'info'">
                    {{ device.is_public ? '是' : '否' }}
                  </el-tag>
                </el-descriptions-item>
                <el-descriptions-item label="有流">
                  <el-tag size="small" :type="device.has_stream ? 'success' : 'info'">
                    {{ device.has_stream ? '是' : '否' }}
                  </el-tag>
                </el-descriptions-item>
                <el-descriptions-item label="创建时间">{{ formatTime(device.created_at) }}</el-descriptions-item>
                <el-descriptions-item label="推流地址" v-if="device.push_urls?.length">
                  <div v-for="(url, i) in device.push_urls" :key="i" class="url-item">
                    <el-tag size="small" type="warning">{{ url.protocol }}</el-tag>
                    <span class="url-text">{{ url.url }}</span>
                  </div>
                </el-descriptions-item>
                <el-descriptions-item label="拉流地址" v-if="device.pull_urls?.length">
                  <div v-for="(url, i) in device.pull_urls" :key="i" class="url-item">
                    <el-tag size="small" type="info">{{ url.protocol }}</el-tag>
                    <span class="url-text">{{ url.url }}</span>
                  </div>
                </el-descriptions-item>
              </el-descriptions>
            </div>
            <el-skeleton v-else animated :rows="3" />
          </el-card>

          <el-card shadow="never" style="margin-top: 16px" v-if="stream.channel_tag">
            <template #header>
              <span>通道信息</span>
              <el-button text size="small" style="float: right" @click="$router.push(`/channels/${stream.device_tag}/${stream.channel_tag}`)">
                查看详情
              </el-button>
            </template>
            <div v-if="channel">
              <el-descriptions :column="1" border size="small">
                <el-descriptions-item label="通道名称">{{ channel.name || '-' }}</el-descriptions-item>
                <el-descriptions-item label="通道标识">{{ channel.channel_tag }}</el-descriptions-item>
                <el-descriptions-item label="状态">
                  <span class="status-dot" :class="channel.status?.toLowerCase()" />
                  {{ channel.status || '-' }}
                </el-descriptions-item>
                <el-descriptions-item label="媒体服务器">{{ channel.media_server_name || '-' }}</el-descriptions-item>
                <el-descriptions-item label="创建时间">{{ formatTime(channel.created_at) }}</el-descriptions-item>
              </el-descriptions>
            </div>
            <el-skeleton v-else animated :rows="3" />
          </el-card>
        </el-col>
      </el-row>

      <el-skeleton v-else-if="loading" animated :rows="8" />

      <el-empty v-else description="流不存在" />

      <el-dialog v-model="showPresetDialog" title="保存预制位" width="400px">
        <el-input v-model="newPresetName" placeholder="例如: 大门入口" />
        <template #footer>
          <el-button @click="showPresetDialog = false">取消</el-button>
          <el-button type="primary" :disabled="!newPresetName.trim()" @click="savePreset">保存</el-button>
        </template>
      </el-dialog>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Refresh, Back, VideoPlay } from '@element-plus/icons-vue'
import VideoPlayer from '../components/VideoPlayer.vue'
import { useStreamStore } from '../stores/streamStore'
import { useDeviceStore } from '../stores/deviceStore'
import { useChannelStore } from '../stores/channelStore'
import * as api from '../api/streams'
import * as channelsApi from '../api/channels'

const route = useRoute()
const router = useRouter()
const streamStore = useStreamStore()
const deviceStore = useDeviceStore()
const channelStore = useChannelStore()

const id = computed(() => Number(route.params.id))
const loading = ref(false)
const stopping = ref(false)
const starting = ref(false)
const previewLoading = ref(false)
const playingUrl = ref('')
const playingType = ref('auto')
const stream = ref(null)
const playLinks = ref(null)
const device = ref(null)
const channel = ref(null)
const playerRef = ref(null)
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

const stateType = (state) => {
  const map = { Active: 'success', Idle: 'info', Starting: 'warning', Stopping: 'warning', Recovering: 'warning', Error: 'danger', Stopped: 'info' }
  return map[state] || 'info'
}

const stateLabel = (state) => {
  const map = { Active: '活跃', Idle: '空闲', Starting: '启动中', Stopping: '停止中', Recovering: '恢复中', Error: '错误', Stopped: '已停止' }
  return map[state] || state
}

const playLinkRows = computed(() => {
  if (!playLinks.value) return []
  const rows = []
  if (playLinks.value.hls) rows.push({ protocol: 'HLS', url: playLinks.value.hls })
  if (playLinks.value.flv) rows.push({ protocol: 'FLV', url: playLinks.value.flv })
  if (playLinks.value.web_flv) rows.push({ protocol: 'WebFLV', url: playLinks.value.web_flv })
  if (playLinks.value.rtsp_signaling) rows.push({ protocol: 'RTSP', url: playLinks.value.rtsp_signaling })
  if (playLinks.value.webrtc) rows.push({ protocol: 'WebRTC', url: playLinks.value.webrtc })
  return rows
})

const protocolMap = { 'HLS': 'hls', 'FLV': 'flv', 'WebFLV': 'wsflv', 'RTSP': 'rtsp', 'WebRTC': 'webrtc' }

const playLinksFromStore = computed(() => playLinks.value)

const supportPtz = computed(() => {
  if (!device.value) return false
  const proto = (device.value.protocol || '').toLowerCase()
  return proto === 'onvif' || proto === 'gb28181'
})

async function fetchData() {
  loading.value = true
  try {
    const [streamData, linksData] = await Promise.all([
      api.getStream(id.value),
      api.getStreamPlayLinks(id.value),
    ])
    stream.value = streamData
    playLinks.value = linksData

    if (stream.value?.device_tag) {
      const dev = await deviceStore.fetchDevice(stream.value.device_tag)
      device.value = dev || null
    }

    if (stream.value?.device_tag && stream.value?.channel_tag) {
      try {
        const ch = await channelsApi.getChannel(stream.value.device_tag, stream.value.channel_tag)
        channel.value = ch || null
      } catch {
        channel.value = null
      }
    }

    if (supportPtz.value && stream.value?.device_tag && stream.value?.channel_tag) {
      await fetchPresets()
    }
  } catch (e) {
    ElMessage.error('加载失败: ' + (e?.message || e))
  } finally {
    loading.value = false
  }
}

async function startPreview() {
  if (!playLinks.value?.hls && !playLinks.value?.flv && !playLinks.value?.web_flv) {
    ElMessage.warning('无可用播放链接')
    return
  }
  playingUrl.value = playLinks.value.hls || playLinks.value.flv || playLinks.value.web_flv
  if (playingUrl.value.includes('.m3u8')) {
    playingType.value = 'hls'
  } else if (playingUrl.value.startsWith('ws://') || playingUrl.value.startsWith('wss://')) {
    playingType.value = 'wsflv'
  } else {
    playingType.value = 'flv'
  }
}

function playProtocol(protocol, url) {
  playingUrl.value = url
  playingType.value = protocolMap[protocol] || 'auto'
}

async function copyUrl(url) {
  try {
    await navigator.clipboard.writeText(url)
  } catch {
    const el = document.createElement('textarea')
    el.value = url
    document.body.appendChild(el)
    el.select()
    document.execCommand('copy')
    document.body.removeChild(el)
  }
  ElMessage.success('已复制')
}

async function copyAll() {
  const text = playLinkRows.value.map(r => `${r.protocol}: ${r.url}`).join('\n')
  await copyUrl(text)
}

async function fetchPresets() {
  try {
    const { device_tag, channel_tag } = stream.value
    presets.value = await channelStore.getChannelPtzPresets(device_tag, channel_tag)
  } catch {
    presets.value = []
  }
}

async function ptzControl(cmd) {
  clearTimeout(ptzStopTimer)
  try {
    const { device_tag, channel_tag } = stream.value
    await channelStore.channelPtzControl(device_tag, channel_tag, cmd, ptzSpeed.value)
  } catch (e) {
    console.error('PTZ control failed:', e)
  }
}

function ptzStop() {
  ptzStopTimer = setTimeout(async () => {
    try {
      const { device_tag, channel_tag } = stream.value
      await channelStore.channelPtzControl(device_tag, channel_tag, 'stop', ptzSpeed.value)
    } catch {}
  }, 100)
}

async function savePreset() {
  if (!newPresetName.value.trim()) return
  try {
    const { device_tag, channel_tag } = stream.value
    await channelStore.createChannelPtzPreset(device_tag, channel_tag, newPresetName.value.trim())
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
    const { device_tag, channel_tag } = stream.value
    await channelStore.channelPtzControl(device_tag, channel_tag, 'goto_preset', ptzSpeed.value, preset.token)
  } catch {
    ElMessage.error('调用预制位失败')
  }
}

async function removePreset(preset) {
  try {
    const { device_tag, channel_tag } = stream.value
    await channelStore.deleteChannelPtzPreset(device_tag, channel_tag, preset.token)
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
    const { device_tag, channel_tag } = stream.value
    channelStore.channelPtzControl(device_tag, channel_tag, {
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
  
  const { device_tag, channel_tag } = stream.value
  channelStore.channelPtzControl(device_tag, channel_tag, 'stop', ptzSpeed.value)
}

async function fetchPtzStatus() {
  if (!stream.value?.device_tag || !stream.value?.channel_tag) return
  try {
    const status = await channelStore.getChannelPtzStatus(stream.value.device_tag, stream.value.channel_tag)
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

async function confirmStop() {
  try {
    await ElMessageBox.confirm('确定停止此流吗？', '确认', { type: 'warning' })
    stopping.value = true
    await api.stopStream(id.value)
    ElMessage.success('流已停止')
    router.push('/streams')
  } catch (e) {
    if (e !== 'cancel') ElMessage.error('停止失败')
  } finally {
    stopping.value = false
  }
}

async function confirmRestart() {
  try {
    await ElMessageBox.confirm('确定重启此流吗？', '确认', { type: 'warning' })
    starting.value = true
    await streamStore.restartStream(id.value)
    ElMessage.success('流已启动')
    await fetchData()
  } catch (e) {
    if (e !== 'cancel') ElMessage.error('启动失败')
  } finally {
    starting.value = false
  }
}

function formatTime(ts) {
  if (!ts) return '-'
  const d = new Date(ts)
  return d.toLocaleString()
}

onMounted(() => {
  fetchData()
  
  document.addEventListener('mousemove', updateJoystick)
  document.addEventListener('mouseup', stopJoystick)
  document.addEventListener('touchmove', updateJoystick)
  document.addEventListener('touchend', stopJoystick)
  
  if (supportPtz.value) {
    startPtzStatusPolling()
  }
})

onUnmounted(() => {
  if (playerRef.value) {
    playerRef.value.destroy()
  }
  document.removeEventListener('mousemove', updateJoystick)
  document.removeEventListener('mouseup', stopJoystick)
  document.removeEventListener('touchmove', updateJoystick)
  document.removeEventListener('touchend', stopJoystick)
  stopPtzStatusPolling()
})
</script>

<style scoped>
.page-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
.header-left { display: flex; align-items: center; }
.header-right { display: flex; gap: 8px; }
.page-title { font-size: var(--text-xl); font-weight: var(--weight-semibold); color: var(--text-primary); }

.player-with-ptz {
  display: flex;
  gap: 16px;
  align-items: stretch;
}

.player-wrapper {
  flex: 1;
  aspect-ratio: 16 / 9;
  background: var(--video-bg, #000);
  border-radius: var(--radius-base);
  overflow: hidden;
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

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.player-placeholder { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 300px; color: var(--text-muted); gap: var(--space-3); }
.url-text { font-family: var(--font-mono); font-size: var(--text-xs); word-break: break-all; }
.status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: var(--space-1); }
.status-dot.online { background: var(--color-success); }
.status-dot.offline { background: var(--text-muted); }
.url-item { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
</style>
