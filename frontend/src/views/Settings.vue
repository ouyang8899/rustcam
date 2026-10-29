<template>
  <div class="page-container">
    <div class="page-header">
      <h1 class="page-title">设置</h1>
      <div class="page-toolbar">
        <el-button type="primary" :loading="saving" @click="saveServers">保存配置</el-button>
      </div>
    </div>
    <div class="page-body">
      <el-row :gutter="12">
        <el-col :span="12">
          <DataCard>
            <template #header><span>服务器配置</span></template>
            <el-text type="info" size="small" style="margin-bottom: 12px; display: block">从配置文件加载，无法在此页面修改</el-text>
            <el-descriptions :column="1" border size="small">
              <el-descriptions-item label="主机地址">{{ config.server.host }}</el-descriptions-item>
              <el-descriptions-item label="端口">{{ config.server.port }}</el-descriptions-item>
              <el-descriptions-item label="PostgreSQL URL"><code class="mono">{{ config.database.url }}</code></el-descriptions-item>
              <el-descriptions-item label="Redis URL"><code class="mono">{{ config.redis.url }}</code></el-descriptions-item>
            </el-descriptions>
          </DataCard>
        </el-col>

        <el-col :span="12">
          <DataCard>
            <template #header>
              <div style="display: flex; align-items: center; justify-content: space-between">
                <span>媒体服务器</span>
                <el-button size="small" :icon="Plus" @click="addServer">添加服务器</el-button>
              </div>
            </template>
            <el-skeleton animated :loading="loading" :rows="3">
              <template #default>
                <div v-for="(server, index) in config.media_servers.servers" :key="index" style="margin-bottom: 16px; padding-bottom: 16px; border-bottom: 1px solid var(--border)">
                  <div style="display: flex; justify-content: flex-end; margin-bottom: 8px">
                    <el-button size="small" type="danger" plain :icon="Delete" @click="deleteServer(index)">删除</el-button>
                  </div>
                  <el-form label-position="top" size="small">
                    <el-row :gutter="8">
                      <el-col :span="12">
                        <el-form-item label="名称"><el-input v-model="server.name" /></el-form-item>
                      </el-col>
                      <el-col :span="12">
                        <el-form-item label="类型">
                          <el-select v-model="server.server_type" style="width: 100%">
                            <el-option value="zlmediakit" label="ZLMediaKit" />
                            <el-option value="srs" label="SRS" />
                            <el-option value="xiu" label="Xiu" />
                          </el-select>
                        </el-form-item>
                      </el-col>
                    </el-row>
                    <el-form-item label="API URL"><el-input v-model="server.url" placeholder="http://localhost:8090" /></el-form-item>
                    <el-form-item label="API Key"><el-input v-model="server.api_key" type="password" show-password /></el-form-item>
                  </el-form>
                </div>
                <el-empty v-if="config.media_servers.servers.length === 0" description="暂无媒体服务器" :image-size="60" />
              </template>
            </el-skeleton>
          </DataCard>

          <DataCard>
            <template #header>
              <div style="display: flex; align-items: center; justify-content: space-between">
                <span>电子地图配置</span>
                <el-button size="small" type="primary" :loading="savingMap" @click="saveMapConfig">保存地图配置</el-button>
              </div>
            </template>
            <el-form-item label="TMS 瓦片地址（浏览器）">
              <el-input v-model="browserTmsUrl" type="textarea" :rows="3" placeholder="https://tile.openstreetmap.org/{z}/{x}/{y}.png" />
            </el-form-item>
            <el-form-item label="TMS 瓦片地址（客户端）">
              <el-input v-model="clientTmsUrl" type="textarea" :rows="3" placeholder="https://tile.openstreetmap.org/{z}/{x}/{y}.png" />
            </el-form-item>
            <el-form-item label="缩放层级（浏览器）">
              <div style="display: flex; align-items: center; gap: 8px">
                <el-input-number v-model="browserMinZoom" :min="0" :max="24" :step="1" controls-position="right" style="width: 120px" />
                <span>至</span>
                <el-input-number v-model="browserMaxZoom" :min="0" :max="24" :step="1" controls-position="right" style="width: 120px" />
              </div>
            </el-form-item>
            <el-form-item label="缩放层级（客户端）">
              <div style="display: flex; align-items: center; gap: 8px">
                <el-input-number v-model="clientMinZoom" :min="0" :max="24" :step="1" controls-position="right" style="width: 120px" />
                <span>至</span>
                <el-input-number v-model="clientMaxZoom" :min="0" :max="24" :step="1" controls-position="right" style="width: 120px" />
              </div>
            </el-form-item>
            <el-form-item label="WFS 服务地址（共享）">
              <el-input v-model="mapWfsUrl" placeholder="http://localhost:8080/geoserver/ows?service=WFS&version=2.0.0&request=GetFeature" />
            </el-form-item>
            <el-form-item label="自动定位事件">
              <el-switch v-model="mapAutoLocate" />
            </el-form-item>
            <el-text type="info" size="small">
              地图配置保存到服务器数据库；客户端启动时获取客户端 TMS 地址并应用到电子地图<br/>
              TMS: 地图底图，使用 {x} {y} {z} 占位符<br/>
              缩放层级: 地图与瓦片层允许的最小/最大级别（0–24）<br/>
              WFS: 通道点位服务，图层名 myw:channels，需包含 device_tag、channel_tag、name、status、latitude、longitude 字段
            </el-text>
          </DataCard>
        </el-col>
      </el-row>

      <div v-if="successMsg" class="fixed-toast toast-success">
        {{ successMsg }}
      </div>
      <div v-if="errorMsg" class="fixed-toast toast-error">
        {{ errorMsg }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, watch } from 'vue'
import { Plus, Delete } from '@element-plus/icons-vue'
import { useSettingsStore } from '../stores/settingsStore'
import { authFetch } from '../utils/authFetch'
import DataCard from '../components/common/DataCard.vue'

const settingsStore = useSettingsStore()
const browserTmsUrl = ref(settingsStore.browserTmsUrl)
const clientTmsUrl = ref(settingsStore.clientTmsUrl)
const mapWfsUrl = ref(settingsStore.mapWfsUrl)
const browserMinZoom = ref(settingsStore.browserMinZoom)
const browserMaxZoom = ref(settingsStore.browserMaxZoom)
const clientMinZoom = ref(settingsStore.clientMinZoom)
const clientMaxZoom = ref(settingsStore.clientMaxZoom)
const mapAutoLocate = ref(settingsStore.mapAutoLocate)

function saveMapAutoLocate() { settingsStore.setMapAutoLocate(mapAutoLocate.value) }

function applyStoreToRefs() {
  browserTmsUrl.value = settingsStore.browserTmsUrl
  clientTmsUrl.value = settingsStore.clientTmsUrl
  mapWfsUrl.value = settingsStore.mapWfsUrl
  browserMinZoom.value = settingsStore.browserMinZoom
  browserMaxZoom.value = settingsStore.browserMaxZoom
  clientMinZoom.value = settingsStore.clientMinZoom
  clientMaxZoom.value = settingsStore.clientMaxZoom
}

async function saveMapConfig() {
  savingMap.value = true; errorMsg.value = ''
  if (browserMinZoom.value > browserMaxZoom.value) {
    errorMsg.value = '浏览器最小层级不能大于最大层级'; savingMap.value = false
    setTimeout(() => { errorMsg.value = '' }, 5000); return
  }
  if (clientMinZoom.value > clientMaxZoom.value) {
    errorMsg.value = '客户端最小层级不能大于最大层级'; savingMap.value = false
    setTimeout(() => { errorMsg.value = '' }, 5000); return
  }
  try {
    settingsStore.browserTmsUrl = browserTmsUrl.value
    settingsStore.clientTmsUrl = clientTmsUrl.value
    settingsStore.mapWfsUrl = mapWfsUrl.value
    settingsStore.browserMinZoom = browserMinZoom.value
    settingsStore.browserMaxZoom = browserMaxZoom.value
    settingsStore.clientMinZoom = clientMinZoom.value
    settingsStore.clientMaxZoom = clientMaxZoom.value
    await settingsStore.saveMapSettings()
    applyStoreToRefs()
    successMsg.value = '地图配置已保存到服务器'; setTimeout(() => { successMsg.value = '' }, 3000)
  } catch (e) {
    console.error('Failed to save map settings:', e); errorMsg.value = '保存地图配置失败'; setTimeout(() => { errorMsg.value = '' }, 5000)
  } finally {
    savingMap.value = false
  }
}

const config = ref({
  server: { host: '0.0.0.0', port: 8080 },
  database: { url: 'postgres://postgres:postgres@localhost:5432/rustcam' },
  redis: { url: 'redis://localhost:6379' },
  media_servers: { servers: [] }
})

const loading = ref(false)
const saving = ref(false)
const savingMap = ref(false)
const successMsg = ref('')
const errorMsg = ref('')

onMounted(async () => {
  await settingsStore.loadMapSettings()
  applyStoreToRefs()
  await fetchServers()
})

watch(mapAutoLocate, () => saveMapAutoLocate())

async function fetchServers() {
  loading.value = true
  try {
    const res = await authFetch('/api/v1/servers')
    const data = await res.json()
    if (data.code === 200) {
      config.value.media_servers.servers = data.data.map(s => ({
        id: s.id, server_tag: s.server_tag, name: s.name, url: s.url, api_key: s.api_key,
        server_type: s.server_type, weight: s.weight, enabled: s.enabled
      }))
    }
  } catch (e) { console.error('Failed to fetch servers:', e); errorMsg.value = '加载媒体服务器失败'; setTimeout(() => { errorMsg.value = '' }, 5000) }
  finally { loading.value = false }
}

function addServer() { config.value.media_servers.servers.push({ name: '', url: '', api_key: '', server_type: 'zlmediakit', weight: 100, enabled: true }) }
function deleteServer(index) { config.value.media_servers.servers.splice(index, 1) }

async function saveServers() {
  saving.value = true; errorMsg.value = ''
  try {
    for (const server of config.value.media_servers.servers) {
      if (!server.server_tag) {
        throw new Error('缺少 server_tag，无法保存')
      }
      // API 路径参数为 server_tag（不是数字 id）
      const res = await authFetch(`/api/v1/servers/${encodeURIComponent(server.server_tag)}`, {
        method: 'PUT',
        body: JSON.stringify({
          name: server.name,
          url: server.url,
          api_key: server.api_key,
          server_type: server.server_type,
          weight: server.weight,
          enabled: server.enabled,
          server_tag: server.server_tag,
        }),
      })
      // authFetch 不会因 HTTP 错误状态抛异常，必须显式检查
      if (!res.ok) {
        let detail = ''
        try {
          const body = await res.json()
          detail = body.message || body.msg || ''
        } catch { /* 忽略解析失败 */ }
        throw new Error(detail || `HTTP ${res.status}`)
      }
    }
    successMsg.value = '保存成功'; setTimeout(() => { successMsg.value = '' }, 3000)
    await fetchServers()
  } catch (e) { console.error('Failed to save servers:', e); errorMsg.value = '保存媒体服务器失败'; setTimeout(() => { errorMsg.value = '' }, 5000) }
  finally { saving.value = false }
}
</script>

<style scoped>
.mono { font-family: var(--font-mono); }

.fixed-toast {
  position: fixed;
  left: 50%;
  bottom: 20px;
  transform: translateX(-50%);
  z-index: 3000;
  padding: 10px 20px;
  border-radius: 4px;
  color: #fff;
  font-size: 14px;
  line-height: 20px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  pointer-events: none;
  max-width: 80vw;
}

.toast-success { background-color: #67c23a; }
.toast-error { background-color: #f56c6c; }
</style>
