<template>
  <PageView title="设置">
    <el-row :gutter="12">
      <el-col :span="8">
        <DataCard>
          <template #header><span>服务器配置</span></template>
          <el-text type="info" size="small" style="margin-bottom: 12px; display: block">读取自服务端 config.toml，密码默认隐藏</el-text>
          <el-descriptions :column="1" border size="small">
            <el-descriptions-item label="主机地址">{{ config.server.host }}</el-descriptions-item>
            <el-descriptions-item label="端口">{{ config.server.port }}</el-descriptions-item>
            <el-descriptions-item label="PostgreSQL URL">
              <code class="mono">{{ config.database.url }}</code>
              <el-button link type="primary" :icon="showSecret ? Hide : View" @click="toggleSecret">
                {{ showSecret ? '隐藏' : '显示' }}
              </el-button>
            </el-descriptions-item>
            <el-descriptions-item label="Redis URL">
              <code class="mono">{{ config.redis.url }}</code>
            </el-descriptions-item>
          </el-descriptions>
        </DataCard>
      </el-col>

      <el-col :span="16">
        <DataCard>
          <template #header>
            <div style="display: flex; align-items: center; justify-content: space-between">
              <span>电子地图配置</span>
              <el-button size="small" type="primary" :loading="savingMap" @click="saveMapConfig">保存地图配置</el-button>
            </div>
          </template>
          <el-form-item label="底图图层">
            <el-tabs v-model="activeLayerTab" style="width: 100%">
              <el-tab-pane label="浏览器" name="browser">
                <MapLayerTable
                  :layers="browserLayers"
                  @add="openLayerDialog('browser')"
                  @edit="(index) => openLayerDialog('browser', index)"
                  @remove="(index) => removeLayer('browser', index)"
                />
              </el-tab-pane>
              <el-tab-pane label="客户端" name="client">
                <MapLayerTable
                  :layers="clientLayers"
                  @add="openLayerDialog('client')"
                  @edit="(index) => openLayerDialog('client', index)"
                  @remove="(index) => removeLayer('client', index)"
                />
              </el-tab-pane>
            </el-tabs>
          </el-form-item>
          <el-form-item label="WFS 服务地址（共享）">
            <el-input v-model="mapWfsUrl" placeholder="http://localhost:8080/geoserver/ows?service=WFS&version=2.0.0&request=GetFeature" />
          </el-form-item>
          <el-form-item label="WFS 出图投影（srsName）">
            <el-select v-model="mapWfsSrs" style="width: 280px" filterable allow-create default-first-option>
              <el-option v-for="opt in SRS_OPTIONS" :key="opt.value" :label="opt.label" :value="opt.value" />
            </el-select>
          </el-form-item>
          <el-form-item label="自动定位事件">
            <el-switch v-model="mapAutoLocate" />
          </el-form-item>
          <el-text type="info" size="small">
            地图配置保存到服务器数据库；客户端启动时获取客户端图层并应用到电子地图<br/>
            瓦片地址: 使用 {x} {y} {z} 占位符；含 {-y} 按 TMS 处理<br/>
            维度: 2D = 平面地图（只支持 EPSG:3857），3D = 三维地球（支持 EPSG:3857 / 4326 / 4490）<br/>
            SRS: 瓦片服务自身的投影。不被该端支持的投影会被跳过并报错<br/>
            协议: 决定瓦片地址怎么拼、行号怎么算；留空时按地址自动识别（含 {quadkey}→Quadkey、TILEMATRIX→WMTS、{-y}→TMS）<br/>
            格式/瓦片尺寸: 影像格式与瓦片像素尺寸；512px 的图源必须显式选 512，否则会按 256 渲染变糊<br/>
            缩放层级: 跟随图层自身的层级范围（0–24），无需单独设置；2D 与 3D 各按其图层的范围显示<br/>
            WFS: 通道点位服务，图层名 rustcam:channels，需包含 device_tag、channel_tag、name、status、latitude、longitude 字段
          </el-text>
        </DataCard>
      </el-col>
    </el-row>

    <AppDialog v-model="layerDialogVisible" :title="layerDialogTitle" size="lg" draggable>
      <el-form label-width="88px" label-position="right">
        <el-row :gutter="12">
          <el-col :span="12">
            <el-form-item label="名称" required>
              <el-input v-model="layerForm.name" placeholder="影像底图" />
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="维度" required>
              <el-radio-group v-model="layerForm.dim">
                <el-radio-button value="2d">2D 平面</el-radio-button>
                <el-radio-button value="3d">3D 地球</el-radio-button>
              </el-radio-group>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="协议" required>
              <el-select v-model="layerForm.protocol" style="width: 100%" @change="onProtocolChange(layerForm)">
                <el-option v-for="opt in PROTOCOL_OPTIONS" :key="opt.value" :label="opt.label" :value="opt.value" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="SRS" required>
              <el-select v-model="layerForm.srs" style="width: 100%" filterable allow-create default-first-option>
                <el-option v-for="opt in SRS_OPTIONS" :key="opt.value" :label="opt.label" :value="opt.value" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="24">
            <el-form-item label="瓦片地址" required>
              <el-input v-model="layerForm.url" :placeholder="protocolInfo(layerForm.protocol).placeholder" />
              <div class="form-hint">{{ protocolInfo(layerForm.protocol).note }}</div>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="层级" required>
              <div class="zoom-range">
                <el-input-number v-model="layerForm.min_zoom" :min="0" :max="24" controls-position="right" />
                <span>-</span>
                <el-input-number v-model="layerForm.max_zoom" :min="0" :max="24" controls-position="right" />
              </div>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="格式">
              <el-select v-model="layerForm.format" style="width: 100%">
                <el-option v-for="fmt in FORMAT_OPTIONS" :key="fmt" :label="fmt" :value="fmt" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="瓦片尺寸">
              <el-select v-model="layerForm.tile_size" style="width: 100%">
                <el-option v-for="size in TILE_SIZE_OPTIONS" :key="size" :label="`${size}px`" :value="size" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="Bing Key">
              <el-input v-model="layerForm.bing_key" type="password" show-password placeholder="3D 端官方 Bing 需要" />
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="影像集">
              <el-select v-model="layerForm.imagery_set" style="width: 100%">
                <el-option v-for="set in IMAGERY_SET_OPTIONS" :key="set" :label="set" :value="set" />
              </el-select>
            </el-form-item>
          </el-col>
          <el-col :span="12">
            <el-form-item label="启用">
              <el-switch v-model="layerForm.enabled" />
            </el-form-item>
          </el-col>
        </el-row>
      </el-form>
      <template #footer>
        <el-button @click="layerDialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="savingLayer" @click="saveLayer">保存</el-button>
      </template>
    </AppDialog>
  </PageView>
</template>

<script setup>
import { ref, computed, onMounted, watch } from 'vue'
import { ElMessageBox } from 'element-plus'
import { View, Hide } from '@element-plus/icons-vue'
import { useSettingsStore } from '../stores/settingsStore'
import { getSystemConfig, getSystemConfigSecret } from '../api/system'
import {
  TILE_PROTOCOLS,
  detectFormat,
  detectProtocol,
  protocolInfo,
  protocolMatchesUrl,
} from '../utils/mapTiles'
import DataCard from '../components/common/DataCard.vue'
import PageView from '../components/common/PageView.vue'
import AppDialog from '../components/common/AppDialog.vue'
import MapLayerTable from './settings/MapLayerTable.vue'
import { useToast } from '../composables/useToast'

const toast = useToast()
const settingsStore = useSettingsStore()
const browserLayers = ref(cloneLayers(settingsStore.browserLayers))
const clientLayers = ref(cloneLayers(settingsStore.clientLayers))
const mapWfsUrl = ref(settingsStore.mapWfsUrl)
const mapWfsSrs = ref(settingsStore.mapWfsSrs || 'EPSG:4326')
const activeLayerTab = ref('browser')
const mapAutoLocate = ref(settingsStore.mapAutoLocate)

const layerDialogVisible = ref(false)
const layerDialogTarget = ref('browser')
const layerDialogIndex = ref(-1)
const layerForm = ref(emptyLayer())
const savingLayer = ref(false)

const layerDialogTitle = computed(
  () => `${layerDialogIndex.value < 0 ? '添加' : '编辑'}${targetLabel(layerDialogTarget.value)}图层`,
)

const SRS_OPTIONS = [
  { value: 'EPSG:3857', label: 'EPSG:3857 · Web Mercator' },
  { value: 'EPSG:4326', label: 'EPSG:4326 · WGS84 经纬度' },
  { value: 'EPSG:4490', label: 'EPSG:4490 · CGCS2000 经纬度' },
  { value: 'EPSG:4547', label: 'EPSG:4547 · CGCS2000 3 度带 CM117E' },
]

const PROTOCOL_OPTIONS = TILE_PROTOCOLS.map((p) => ({ value: p.value, label: p.label }))
const FORMAT_OPTIONS = ['png', 'jpeg', 'jpg', 'webp']
const TILE_SIZE_OPTIONS = [256, 512]
const IMAGERY_SET_OPTIONS = ['Aerial', 'Road']

function onProtocolChange(row) {
  const info = protocolInfo(row.protocol)
  if (!row.url || !protocolMatchesUrl(row.url, row.protocol)) {
    row.url = info.placeholder.startsWith('http') ? info.placeholder : ''
  }
  row.format = detectFormat(row.url) || row.format || 'png'
}

function cloneLayers(layers) {
  return (Array.isArray(layers) ? layers : []).map((l) => ({
    id: l.id || '',
    name: l.name || '',
    dim: l.dim || '2d',
    url: l.url || '',
    srs: l.srs || 'EPSG:3857',
    protocol: l.protocol || detectProtocol(l.url),
    format: l.format || detectFormat(l.url) || '',
    tile_size: Number(l.tile_size) === 512 ? 512 : 256,
    bing_key: l.bing_key || '',
    imagery_set: l.imagery_set || 'Aerial',
    min_zoom: Number.isInteger(l.min_zoom) ? l.min_zoom : 0,
    max_zoom: Number.isInteger(l.max_zoom) ? l.max_zoom : 19,
    enabled: l.enabled !== false,
  }))
}

function emptyLayer() {
  return {
    id: '',
    name: '',
    dim: '2d',
    url: '',
    srs: 'EPSG:3857',
    protocol: 'xyz',
    format: 'png',
    tile_size: 256,
    bing_key: '',
    imagery_set: 'Aerial',
    min_zoom: 0,
    max_zoom: 19,
    enabled: true,
  }
}

function layersOf(target) {
  return target === 'browser' ? browserLayers.value : clientLayers.value
}

function targetLabel(target) {
  return target === 'browser' ? '浏览器' : '客户端'
}

function openLayerDialog(target, index = -1) {
  layerDialogTarget.value = target
  layerDialogIndex.value = index
  const layers = layersOf(target)
  const current = index >= 0 ? layers[index] : null
  layerForm.value = current ? { ...current } : emptyLayer()
  layerDialogVisible.value = true
}

function validateLayer(layer, label) {
  if (!layer.url || !layer.url.trim()) return `${label}图层缺少瓦片地址`
  if (!layer.srs || !layer.srs.trim()) return `${label}图层缺少 SRS`
  if (!protocolMatchesUrl(layer.url, layer.protocol)) {
    const info = protocolInfo(layer.protocol)
    return `${label}图层的瓦片地址与「${info.label}」协议不匹配，该协议需要 ${info.placeholder}`
  }
  if (layer.min_zoom > layer.max_zoom) return `${label}图层最小层级不能大于最大层级`
  return null
}

function saveMapAutoLocate() { settingsStore.setMapAutoLocate(mapAutoLocate.value) }

function applyStoreToRefs() {
  browserLayers.value = cloneLayers(settingsStore.browserLayers)
  clientLayers.value = cloneLayers(settingsStore.clientLayers)
  mapWfsUrl.value = settingsStore.mapWfsUrl
  mapWfsSrs.value = settingsStore.mapWfsSrs || 'EPSG:4326'
}

async function persistMapConfig() {
  settingsStore.browserLayers = browserLayers.value
  settingsStore.clientLayers = clientLayers.value
  settingsStore.mapWfsUrl = mapWfsUrl.value
  settingsStore.mapWfsSrs = mapWfsSrs.value
  await settingsStore.saveMapSettings()
  applyStoreToRefs()
}

async function saveLayer() {
  const target = layerDialogTarget.value
  const layers = layersOf(target)
  const form = { ...layerForm.value }
  const error = validateLayer(form, targetLabel(target))
  if (error) {
    toast.error(error)
    return
  }
  savingLayer.value = true
  const backup = layers.map((l) => ({ ...l }))
  try {
    if (layerDialogIndex.value >= 0) layers.splice(layerDialogIndex.value, 1, form)
    else layers.push(form)
    await persistMapConfig()
    layerDialogVisible.value = false
    toast.success('图层已保存')
  } catch (e) {
    layers.splice(0, layers.length, ...backup)
    console.error('Failed to save map layer:', e)
    toast.error('保存图层失败: ' + (e?.message || e))
  } finally {
    savingLayer.value = false
  }
}

async function removeLayer(target, index) {
  const layers = layersOf(target)
  const layer = layers[index]
  if (!layer) return
  try {
    await ElMessageBox.confirm(
      `确定删除${targetLabel(target)}图层「${layer.name || '未命名图层'}」吗？`,
      '确认删除',
      { type: 'warning', confirmButtonText: '确定', cancelButtonText: '取消' },
    )
  } catch {
    return
  }
  const backup = layers.map((l) => ({ ...l }))
  layers.splice(index, 1)
  try {
    await persistMapConfig()
    toast.success('图层已删除')
  } catch (e) {
    layers.splice(0, layers.length, ...backup)
    console.error('Failed to remove map layer:', e)
    toast.error('删除图层失败: ' + (e?.message || e))
  }
}

async function saveMapConfig() {
  savingMap.value = true
  for (const [label, layers] of [['浏览器', browserLayers.value], ['客户端', clientLayers.value]]) {
    for (const layer of layers) {
      const error = validateLayer(layer, label)
      if (error) {
        toast.error(error)
        savingMap.value = false
        return
      }
    }
  }
  try {
    await persistMapConfig()
    toast.success('地图配置已保存到服务器')
  } catch (e) {
    console.error('Failed to save map settings:', e); toast.error('保存地图配置失败')
  } finally {
    savingMap.value = false
  }
}

const config = ref({
  server: { host: '', port: '' },
  database: { url: '' },
  redis: { url: '' },
})

const showSecret = ref(false)
const savingMap = ref(false)

async function fetchSystemConfig() {
  try {
    const data = await getSystemConfig()
    if (!data) return
    config.value = {
      server: data.server || config.value.server,
      database: data.database || config.value.database,
      redis: data.redis || config.value.redis,
    }
  } catch (e) {
    console.error('Failed to fetch system config:', e)
    toast.error('读取服务器配置失败')
  }
}

async function toggleSecret() {
  if (showSecret.value) {
    showSecret.value = false
    return
  }
  try {
    const data = await getSystemConfigSecret()
    if (data) {
      if (data.database?.url) config.value.database.url = data.database.url
      if (data.redis?.url) config.value.redis.url = data.redis.url
      showSecret.value = true
    }
  } catch (e) {
    console.error('Failed to reveal system config secret:', e)
    toast.error('无权限查看连接串密码')
  }
}

onMounted(async () => {
  await settingsStore.loadMapSettings()
  applyStoreToRefs()
  await fetchSystemConfig()
})

watch(mapAutoLocate, () => saveMapAutoLocate())
</script>

<style scoped>
.mono { font-family: var(--font-mono); }

.form-hint {
  font-size: var(--text-xs);
  color: var(--text-muted);
  line-height: 1.5;
  margin-top: 2px;
}

.zoom-range {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.zoom-range .el-input-number { width: 120px; }
</style>
