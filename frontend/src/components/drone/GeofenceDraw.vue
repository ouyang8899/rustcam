<!-- @ts-check -->
<template>
  <div class="fence-draw">
    <div ref="mapEl" class="fence-draw__map" />
  </div>
</template>

<script setup>
// @ts-check
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Map as MapLibreMap, NavigationControl, setWorkerUrl } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { TerraDraw, TerraDrawCircleMode, TerraDrawPolygonMode } from 'terra-draw'
import { TerraDrawMapLibreGLAdapter } from 'terra-draw-maplibre-gl-adapter'
import { useSettingsStore } from '../../stores/settingsStore'
import { FENCE_COLORS, fenceColors, fenceRule } from '../../utils/fenceStyle'
import { buildRasterSource, selectLayers, viewZoomRange } from '../../utils/mapTiles'
import { useToast } from '../../composables/useToast'

const toast = useToast()

setWorkerUrl(`${import.meta.env.BASE_URL}maplibre/maplibre-gl-worker.mjs`)

const DEFAULT_TMS_URL = 'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'
const DEFAULT_VIEW = { lat: 31.2304, lon: 121.4737, zoom: 10 }

const props = defineProps({
  /** @type {{ lat: number, lon: number, zoom?: number }} */
  center: { type: Object, default: () => ({ lat: 31.2304, lon: 121.4737, zoom: 10 }) },
  fences: { type: Array, default: () => [] },
  initialGeometry: { type: Object, default: null },
  /** @type {'polygon' | 'circle'} */
  shape: { type: String, default: 'polygon' },
  rule: { type: String, default: 'no_fly' },
  initialCircle: { type: Object, default: null },
})

const emit = defineEmits(['change'])

const settingsStore = useSettingsStore()
const mapEl = ref(null)
/** @type {import('vue').Ref<any>} */
const geometry = ref(null)
let lastEmitted = ''
let syncingInitial = false
let emitTimer = null
/** @type {any} */
let map = null
/** @type {any} */
let draw = null
let currentRule = fenceRule(props.rule)

function drawStyles() {
  return {
    fillColor: () => fenceColors(currentRule).fill,
    outlineColor: () => fenceColors(currentRule).outline,
    fillOpacity: 0.3,
    outlineOpacity: 1,
    outlineWidth: 3,
  }
}

function canonicalGeometry(geo) {
  if (!geo || geo.type !== 'Polygon') return null
  return {
    type: 'Polygon',
    coordinates: geo.coordinates.map((ring) =>
      ring.map(([lon, lat]) => [Number(Number(lon).toFixed(6)), Number(Number(lat).toFixed(6))])
    ),
  }
}

function newId() {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `fence-${Date.now()}-${Math.round(Math.random() * 1e6)}`
}

/**
 * @param {unknown} value
 * @returns {any}
 */
function parseGeometry(value) {
  if (!value) return null
  try {
    const geo = typeof value === 'string' ? JSON.parse(value) : value
    if (!geo) return null
    if (geo.type === 'Feature') return geo.geometry || null
    if (geo.type === 'Polygon' || geo.type === 'MultiPolygon') return geo
    return null
  } catch (e) {
    return null
  }
}

function circleRing(lon, lat, radiusM, steps = 64) {
  const R = 6378137
  const d = radiusM / R
  const lat1 = (lat * Math.PI) / 180
  const lon1 = (lon * Math.PI) / 180
  const ring = []
  for (let i = 0; i <= steps; i++) {
    const brg = (i / steps) * 2 * Math.PI
    const lat2 = Math.asin(
      Math.sin(lat1) * Math.cos(d) + Math.cos(lat1) * Math.sin(d) * Math.cos(brg)
    )
    const lon2 =
      lon1 +
      Math.atan2(
        Math.sin(brg) * Math.sin(d) * Math.cos(lat1),
        Math.cos(d) - Math.sin(lat1) * Math.sin(lat2)
      )
    ring.push([(lon2 * 180) / Math.PI, (lat2 * 180) / Math.PI])
  }
  return ring
}

function fenceCollection() {
  const features = []
  for (const f of props.fences || []) {
    let geo = parseGeometry(f.geojson)
    const cLon = Number(f.center_lon)
    const cLat = Number(f.center_lat)
    const radius = Number(f.radius_m)
    if (
      !geo &&
      f.shape === 'circle' &&
      Number.isFinite(cLon) &&
      Number.isFinite(cLat) &&
      radius > 0
    ) {
      geo = { type: 'Polygon', coordinates: [circleRing(cLon, cLat, radius)] }
    }
    if (!geo) continue
    features.push({
      type: 'Feature',
      id: f.id,
      properties: { name: f.name, type: fenceRule(f.type) },
      geometry: geo,
    })
  }
  return { type: 'FeatureCollection', features }
}

function addBasemap() {
  if (!map || map.getSource('basemap')) return

  const layers = selectLayers(settingsStore.browserLayers, '2d')
  let source = null
  let chosen = null
  for (const layer of layers) {
    const candidate = buildRasterSource(layer)
    if (candidate) {
      source = candidate
      chosen = layer
      break
    }
    console.warn('[GeofenceDraw] 底图图层在该端不可用，已跳过:', layer.name || layer.url, layer.srs)
    toast.warning(
      `底图图层「${layer.name || layer.url}」的投影 ${layer.srs} 不被浏览器 2D 支持（仅 EPSG:3857），已跳过`,
    )
  }

  const { minZoom, maxZoom } = viewZoomRange(chosen)

  if (!source) {
    source = {
      type: 'raster',
      tiles: [DEFAULT_TMS_URL],
      tileSize: 256,
      minzoom: minZoom,
      maxzoom: maxZoom,
      scheme: 'xyz',
    }
  }

  map.addSource('basemap', source)
  map.addLayer({ id: 'basemap-layer', type: 'raster', source: 'basemap', minzoom: minZoom, maxzoom: maxZoom })
  map.setMinZoom(minZoom)
  map.setMaxZoom(maxZoom)
}

function addFenceLayers() {
  if (!map || map.getSource('drone-fences')) return
  map.addSource('drone-fences', { type: 'geojson', data: fenceCollection() })
  map.addLayer({
    id: 'drone-fences-fill',
    type: 'fill',
    source: 'drone-fences',
    paint: {
      'fill-color': [
        'case',
        ['==', ['get', 'type'], 'allowed'],
        FENCE_COLORS.allowed.fill,
        FENCE_COLORS.no_fly.fill,
      ],
      'fill-opacity': 0.18,
    },
  })
  map.addLayer({
    id: 'drone-fences-line',
    type: 'line',
    source: 'drone-fences',
    paint: {
      'line-color': [
        'case',
        ['==', ['get', 'type'], 'allowed'],
        FENCE_COLORS.allowed.outline,
        FENCE_COLORS.no_fly.outline,
      ],
      'line-width': 2,
    },
  })}

function syncFences() {
  const source = map && map.getSource('drone-fences')
  if (source) source.setData(fenceCollection())
}

function emitGeometry() {
  const json = geometry.value ? JSON.stringify(geometry.value) : ''
  if (json === lastEmitted) return
  lastEmitted = json
  emit('change', geometry.value)
}

function flushEmitTimer() {
  if (emitTimer) {
    clearTimeout(emitTimer)
    emitTimer = null
  }
}

function syncGeometry(immediate = false, report = true) {
  if (!draw) return
  const polygons = draw
    .getSnapshot()
    .filter(
      (feature) =>
        feature.geometry &&
        feature.geometry.type === 'Polygon' &&
        !feature.properties?.currentlyDrawing
    )
  const geo = polygons.length > 0 ? polygons[polygons.length - 1].geometry : null
  geometry.value = canonicalGeometry(geo)
  if (syncingInitial) return
  if (!report) {
    flushEmitTimer()
    lastEmitted = geometry.value ? JSON.stringify(geometry.value) : ''
    return
  }
  if (immediate) {
    flushEmitTimer()
    emitGeometry()
    return
  }
  flushEmitTimer()
  emitTimer = setTimeout(() => {
    emitTimer = null
    emitGeometry()
  }, 120)
}

function clear(silent = false) {
  if (!draw) return
  flushEmitTimer()
  const hadGeometry = lastEmitted !== ''
  syncingInitial = true
  try {
    draw.clear()
  } finally {
    syncingInitial = false
  }
  geometry.value = null
  lastEmitted = ''
  if (!silent && hadGeometry) emit('change', null)
}

defineExpose({ clear })

function applyShape() {
  if (!draw) return
  const mode = props.shape === 'circle' ? 'circle' : 'polygon'
  if (draw.getMode() !== mode) draw.setMode(mode)
}

function finishByContextMenu(e) {
  if (e && typeof e.preventDefault === 'function') e.preventDefault()
  if (!map) return
  map.getCanvas().dispatchEvent(new KeyboardEvent('keyup', { key: 'Enter' }))
}

function syncInitial() {
  if (!draw) return
  let geo = parseGeometry(props.initialGeometry)
  const c = props.initialCircle
  const cLon = Number(c?.lon)
  const cLat = Number(c?.lat)
  const cRadius = Number(c?.radius_m)
  if (
    !geo &&
    props.shape === 'circle' &&
    Number.isFinite(cLon) &&
    Number.isFinite(cLat) &&
    cRadius > 0
  ) {
    geo = { type: 'Polygon', coordinates: [circleRing(cLon, cLat, cRadius)] }
  }
  geo = canonicalGeometry(geo)
  const json = geo ? JSON.stringify(geo) : ''
  if (json === lastEmitted) return
  syncingInitial = true
  try {
    draw.clear()
    if (geo) {
      draw.addFeatures([
        {
          type: 'Feature',
          id: newId(),
          properties: { mode: props.shape === 'circle' ? 'circle' : 'polygon' },
          geometry: geo,
        },
      ])
    }
  } catch (e) {
    console.error('[fence] 同步二维围栏失败', e)
  } finally {
    syncingInitial = false
  }
  syncGeometry(true, false)
}

watch(() => props.fences, syncFences, { deep: true })
watch(
  () => props.rule,
  (rule) => {
    currentRule = fenceRule(rule)
    if (!draw) return
    try {
      draw.updateModeOptions('polygon', { styles: drawStyles() })
      draw.updateModeOptions('circle', { styles: drawStyles() })
    } catch (e) {
      console.error('[fence] 更新围栏配色失败', e)
    }
  }
)
watch(() => props.initialGeometry, syncInitial, { deep: true })
watch(() => props.initialCircle, syncInitial, { deep: true })
watch(() => props.shape, () => {
  applyShape()
  syncInitial()
})
watch(
  () => props.center,
  (center) => {
    if (!map || !center) return
    map.flyTo({
      center: [center.lon, center.lat],
      zoom: center.zoom ?? map.getZoom(),
      duration: 600,
    })
  },
  { deep: true }
)

onMounted(async () => {
  if (!mapEl.value) return

  await settingsStore.loadMapSettings()

  map = new MapLibreMap({
    container: mapEl.value,
    center: [props.center.lon, props.center.lat],
    zoom: props.center.zoom ?? DEFAULT_VIEW.zoom,
    style: {
      version: 8,
      sources: {},
      layers: [
        {
          id: 'background',
          type: 'background',
          paint: { 'background-color': '#0f172a' },
        },
      ],
    },
  })
  map.addControl(new NavigationControl({ showCompass: false }), 'top-right')

  map.on('load', () => {
    addBasemap()
    addFenceLayers()

    draw = new TerraDraw({
      adapter: new TerraDrawMapLibreGLAdapter({ map }),
      modes: [
        new TerraDrawPolygonMode({ styles: drawStyles() }),
        new TerraDrawCircleMode({
          pointerEvents: {
            rightClick: false,
            contextMenu: false,
            leftClick: true,
            onDragStart: true,
            onDrag: true,
            onDragEnd: true,
          },
          styles: drawStyles(),
        }),
      ],
    })
    draw.start()
    applyShape()
    map.on('contextmenu', finishByContextMenu)
    draw.on('finish', (id) => {
      for (const feature of draw.getSnapshot()) {
        if (feature.id !== id) draw.removeFeatures([feature.id])
      }
      syncGeometry(true)
    })
    draw.on('change', () => syncGeometry())
    syncInitial()
  })
})

onBeforeUnmount(() => {
  flushEmitTimer()
  if (map) map.off('contextmenu', finishByContextMenu)
  if (draw) draw.stop()
  if (map) map.remove()
  draw = null
  map = null
})
</script>

<style scoped>
.fence-draw {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.fence-draw__map {
  flex: 1;
  min-height: 320px;
}
.fence-draw__map :deep(.maplibregl-canvas) {
  cursor: crosshair !important;
}
</style>
