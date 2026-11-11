<!-- @ts-check -->
<template>
  <div ref="mapEl" class="drone-map2d" />
</template>

<script setup>
// @ts-check
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { LngLatBounds, Map as MapLibreMap, Marker, NavigationControl, setWorkerUrl } from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { useSettingsStore } from '../../stores/settingsStore'
import { FENCE_COLORS, fenceRule } from '../../utils/fenceStyle'

// MapLibre 通过 import.meta.url 推导 Worker 地址；经 Vite 预打包/分包后该地址并不存在，
// GeoJSON 图层（围栏、航线、回放轨迹）会因 Worker 加载失败而完全不渲染。
// Worker 由 vite.config.js 的 viteStaticCopy 发布到 /maplibre/，这里显式指定。
setWorkerUrl(`${import.meta.env.BASE_URL}maplibre/maplibre-gl-worker.mjs`)

const DEFAULT_TMS_URL =
  'https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}'
const DEFAULT_VIEW = { lat: 31.2304, lon: 121.4737, zoom: 12 }

const props = defineProps({
  /** @type {{ lat: number, lon: number, zoom?: number }} */
  center: { type: Object, default: () => ({ lat: 31.2304, lon: 121.4737, zoom: 12 }) },
  /** @type {Array<any>} */
  drones: { type: Array, default: () => [] },
  selectedId: { type: String, default: '' },
  /** @type {Array<any>} */
  fences: { type: Array, default: () => [] },
  /** @type {Array<any>} 航线点 {lat, lon, alt?} */
  route: { type: Array, default: () => [] },
  /** 航线变化时自动把视野适配到航线 + 当前无人机（实时飞行页使用） */
  autoFitRoute: { type: Boolean, default: false },
  /** @type {Array<any>} 回放轨迹点 {lat, lon, alt?} */
  track: { type: Array, default: () => [] },
  /** @type {number} 回放进度 0..1：已飞过 / 未飞过的轨迹分开着色 */
  trackProgress: { type: Number, default: 0 },
  /** @type {Array<any>} 待提交/草稿点 {lat, lon} */
  draftPoints: { type: Array, default: () => [] },
  /** @type {'none'|'waypoint'} */
  drawMode: { type: String, default: 'none' },
})

const emit = defineEmits(['map-click', 'select', 'ready'])

const settingsStore = useSettingsStore()
const mapEl = ref(null)
/** @type {any} */
let map = null
/** @type {Map<string, { marker: any, el: HTMLElement, key: string }>} */
const droneMarkers = new Map()
/** @type {any} 回放起点/终点标签 */
let trackStartMarker = null
let trackEndMarker = null
/** @type {[number, number][]} 回放轨迹缓存（lon/lat） */
let cachedTrack = []
let lastTrackIndex = -1

function newId() {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
    ? crypto.randomUUID()
    : `id-${Date.now()}-${Math.round(Math.random() * 1e6)}`
}

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

/**
 * 圆形围栏在后端只存圆心与半径，这里按大圆公式离散成多边形用于 2D 回显。
 * @param {number} lon
 * @param {number} lat
 * @param {number} radiusM
 * @param {number} steps
 */
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

function fenceFeatures() {
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
      id: f.id || newId(),
      properties: { name: f.name, type: fenceRule(f.type), shape: f.shape || 'polygon' },
      geometry: geo,
    })
  }
  return { type: 'FeatureCollection', features }
}

/**
 * @param {Array<any>} points
 * @returns {[number, number][]}
 */
function toLngLat(points) {
  const coords = []
  for (const p of points || []) {
    const lon = Array.isArray(p) ? Number(p[0]) : Number(p.lon)
    const lat = Array.isArray(p) ? Number(p[1]) : Number(p.lat)
    if (Number.isFinite(lon) && Number.isFinite(lat)) coords.push([lon, lat])
  }
  return coords
}

function routeCollection() {
  const coords = toLngLat(props.route)
  const features = []
  if (coords.length >= 2) {
    features.push({
      type: 'Feature',
      properties: { kind: 'route-line' },
      geometry: { type: 'LineString', coordinates: coords },
    })
  }
  coords.forEach((c, i) => {
    features.push({
      type: 'Feature',
      properties: { kind: 'route-point', index: i + 1 },
      geometry: { type: 'Point', coordinates: c },
    })
  })
  return { type: 'FeatureCollection', features }
}

function draftCollection() {
  const features = toLngLat(props.draftPoints).map((c) => ({
    type: 'Feature',
    properties: {},
    geometry: { type: 'Point', coordinates: c },
  }))
  return { type: 'FeatureCollection', features }
}

/**
 * 回放轨迹按进度切段：已飞过的部分高亮，未飞过的部分用虚线。
 * @param {number} progress
 */
function trackSegments(progress) {
  const coords = cachedTrack
  const n = coords.length
  if (n === 0) return { done: [], todo: [] }
  const idx =
    n <= 1 ? 1 : Math.max(1, Math.min(n, Math.round(progress * (n - 1)) + 1))
  // 未飞过的段从当前点开始，两段在节点处衔接，避免出现断口。
  return { done: coords.slice(0, idx), todo: coords.slice(Math.max(0, idx - 1)) }
}

/**
 * @param {[number, number][]} coords
 */
function lineCollection(coords) {
  return {
    type: 'FeatureCollection',
    features:
      coords.length >= 2
        ? [
            {
              type: 'Feature',
              properties: {},
              geometry: { type: 'LineString', coordinates: coords },
            },
          ]
        : [],
  }
}

function trackEndsCollection() {
  const features = []
  if (cachedTrack.length > 0) {
    features.push({
      type: 'Feature',
      properties: { kind: 'start' },
      geometry: { type: 'Point', coordinates: cachedTrack[0] },
    })
    if (cachedTrack.length > 1) {
      features.push({
        type: 'Feature',
        properties: { kind: 'end' },
        geometry: { type: 'Point', coordinates: cachedTrack[cachedTrack.length - 1] },
      })
    }
  }
  return { type: 'FeatureCollection', features }
}

/**
 * @param {string} text
 * @param {string} color
 */
function endLabel(text, color) {
  const el = document.createElement('div')
  el.textContent = text
  el.style.cssText = [
    'padding:1px 7px',
    'border-radius:9px',
    'font-size:11px',
    'line-height:16px',
    'color:#fff',
    `background:${color}`,
    'border:1px solid rgba(255,255,255,0.65)',
    'box-shadow:0 2px 6px rgba(0,0,0,0.35)',
    'white-space:nowrap',
    'pointer-events:none',
    'user-select:none',
  ].join(';')
  return el
}

function syncTrackMarkers() {
  if (!map || cachedTrack.length === 0) {
    if (trackStartMarker) {
      trackStartMarker.remove()
      trackStartMarker = null
    }
    if (trackEndMarker) {
      trackEndMarker.remove()
      trackEndMarker = null
    }
    return
  }
  const first = cachedTrack[0]
  const last = cachedTrack[cachedTrack.length - 1]
  if (!trackStartMarker) {
    trackStartMarker = new Marker({ element: endLabel('起点', '#16a34a'), anchor: 'bottom' })
  }
  trackStartMarker.setLngLat(first).addTo(map)
  if (cachedTrack.length > 1) {
    if (!trackEndMarker) {
      trackEndMarker = new Marker({ element: endLabel('终点', '#dc2626'), anchor: 'bottom' })
    }
    trackEndMarker.setLngLat(last).addTo(map)
  } else if (trackEndMarker) {
    trackEndMarker.remove()
    trackEndMarker = null
  }
}

/**
 * 同步回放轨迹。进度只在轨迹点索引变化时才重设 GeoJSON，避免逐帧写图层。
 * @param {boolean} [force] 轨迹本身变化时强制重建缓存
 */
function syncTrack(force = false) {
  if (!map || !map.getSource('drone-track-done')) return
  if (force || cachedTrack.length === 0) {
    cachedTrack = toLngLat(props.track)
    lastTrackIndex = -1
  }
  const n = cachedTrack.length
  const idx =
    n <= 1
      ? 1
      : Math.max(1, Math.min(n, Math.round(props.trackProgress * (n - 1)) + 1))
  if (!force && idx === lastTrackIndex) return
  lastTrackIndex = idx
  const { done, todo } = trackSegments(props.trackProgress)
  const doneSource = map.getSource('drone-track-done')
  if (doneSource) doneSource.setData(lineCollection(done))
  const todoSource = map.getSource('drone-track-todo')
  if (todoSource) todoSource.setData(lineCollection(todo))
  const endsSource = map.getSource('drone-track-ends')
  if (endsSource) endsSource.setData(trackEndsCollection())
  // 起点/终点标记只跟轨迹本身有关，进度推进时无需反复 setLngLat。
  if (force) syncTrackMarkers()
}

function fitTrack() {
  if (!map || !map.getSource('drone-track-done') || cachedTrack.length === 0) return
  const coords = cachedTrack
  if (coords.length === 1) {
    map.flyTo({ center: coords[0], zoom: 15, duration: 500 })
    return
  }
  const bounds = coords.reduce((b, c) => b.extend(c), new LngLatBounds(coords[0], coords[0]))
  map.fitBounds(bounds, {
    padding: { top: 56, bottom: 56, left: 56, right: 56 },
    maxZoom: 16,
    duration: 500,
  })
}

/** 把视野适配到航线与当前无人机，保证“意图航线”一眼可见。 */
function fitRoute() {
  if (!map || !map.getSource('drone-route') || props.route.length === 0) return
  const coords = toLngLat(props.route)
  const d = props.drones.find((x) => x.drone_tag === props.selectedId)
  const t = d?.last_telemetry
  if (t && Number.isFinite(Number(t.lon)) && Number.isFinite(Number(t.lat))) {
    coords.push([Number(t.lon), Number(t.lat)])
  }
  if (coords.length === 1) {
    map.flyTo({ center: coords[0], zoom: 15, duration: 600 })
    return
  }
  const bounds = coords.reduce((b, c) => b.extend(c), new LngLatBounds(coords[0], coords[0]))
  map.fitBounds(bounds, {
    padding: { top: 70, bottom: 70, left: 70, right: 70 },
    maxZoom: 15,
    duration: 600,
  })
}

function addBasemap() {
  if (!map || map.getSource('basemap')) return
  const configured = settingsStore.browserTmsUrl || ''
  const configuredTiles = configured
    .split('\n')
    .map((u) => u.trim())
    .filter((u) => u.includes('{x}') && u.includes('{z}'))
  const tiles = configuredTiles.length > 0 ? configuredTiles : [DEFAULT_TMS_URL]
  const hasNegY = tiles.some((u) => u.includes('{-y}'))
  const normalized = tiles.map((u) => (hasNegY ? u.replace(/\{-y\}/g, '{y}') : u))

  map.addSource('basemap', {
    type: 'raster',
    tiles: normalized,
    tileSize: 256,
    maxzoom: 19,
    scheme: hasNegY ? 'tms' : 'xyz',
  })
  map.addLayer({ id: 'basemap-layer', type: 'raster', source: 'basemap' })
}

function addOverlayLayers() {
  if (!map || map.getSource('drone-fences')) return
  map.addSource('drone-fences', { type: 'geojson', data: fenceFeatures() })
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
  })

  map.addSource('drone-route', { type: 'geojson', data: routeCollection() })
  map.addLayer({
    id: 'drone-route-line',
    type: 'line',
    source: 'drone-route',
    filter: ['==', ['get', 'kind'], 'route-line'],
    paint: {
      'line-color': '#38bdf8',
      'line-width': 2,
      'line-dasharray': [3, 2],
    },
  })
  map.addLayer({
    id: 'drone-route-points',
    type: 'circle',
    source: 'drone-route',
    filter: ['==', ['get', 'kind'], 'route-point'],
    paint: {
      'circle-radius': 4,
      'circle-color': '#0ea5e9',
      'circle-stroke-color': '#e0f2fe',
      'circle-stroke-width': 1.5,
    },
  })

  map.addSource('drone-draft', { type: 'geojson', data: draftCollection() })
  map.addLayer({
    id: 'drone-draft-points',
    type: 'circle',
    source: 'drone-draft',
    paint: {
      'circle-radius': 6,
      'circle-color': '#f59e0b',
      'circle-stroke-color': '#fff7ed',
      'circle-stroke-width': 2,
    },
  })

  // 回放轨迹：未飞过（灰虚线）在下，已飞过（高亮）在上。
  map.addSource('drone-track-todo', { type: 'geojson', data: lineCollection([]) })
  map.addLayer({
    id: 'drone-track-todo-line',
    type: 'line',
    source: 'drone-track-todo',
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': '#94a3b8',
      'line-width': 2,
      'line-dasharray': [2, 2],
      'line-opacity': 0.85,
    },
  })
  map.addSource('drone-track-done', { type: 'geojson', data: lineCollection([]) })
  map.addLayer({
    id: 'drone-track-done-line',
    type: 'line',
    source: 'drone-track-done',
    layout: { 'line-cap': 'round', 'line-join': 'round' },
    paint: {
      'line-color': '#00D4FF',
      'line-width': 4,
      'line-opacity': 0.95,
      'line-blur': 1,
    },
  })
  map.addSource('drone-track-ends', { type: 'geojson', data: trackEndsCollection() })
  map.addLayer({
    id: 'drone-track-ends-points',
    type: 'circle',
    source: 'drone-track-ends',
    paint: {
      'circle-radius': 4,
      'circle-color': ['case', ['==', ['get', 'kind'], 'start'], '#22c55e', '#ef4444'],
      'circle-stroke-color': '#ffffff',
      'circle-stroke-width': 1.5,
    },
  })
}

function syncFences() {
  const source = map && map.getSource('drone-fences')
  if (source) source.setData(fenceFeatures())
}

function syncRoute() {
  const source = map && map.getSource('drone-route')
  if (source) source.setData(routeCollection())
}

function syncDraft() {
  const source = map && map.getSource('drone-draft')
  if (source) source.setData(draftCollection())
}

function statusColor(d, t) {
  if (!d.online) return '#94a3b8'
  return t && t.armed ? '#f97316' : '#22c55e'
}

function droneSvg(color, selected) {
  const ring = selected
    ? '<circle cx="32" cy="32" r="30" fill="none" stroke="' +
      color +
      '" stroke-width="2" stroke-dasharray="5 4" opacity="0.9"/>'
    : ''
  return (
    '<svg xmlns="http://www.w3.org/2000/svg" width="44" height="44" viewBox="0 0 64 64">' +
    ring +
    '<g fill="none" stroke="' +
    color +
    '" stroke-width="3" stroke-linecap="round">' +
    '<path d="M20 20 L44 44 M44 20 L20 44" stroke-width="4"/>' +
    '<circle cx="14" cy="14" r="8"/>' +
    '<circle cx="50" cy="14" r="8"/>' +
    '<circle cx="14" cy="50" r="8"/>' +
    '<circle cx="50" cy="50" r="8"/>' +
    '</g>' +
    '<path d="M32 17 L38 31 L32 28 L26 31 Z" fill="' +
    color +
    '"/>' +
    '<circle cx="32" cy="32" r="6" fill="' +
    color +
    '" stroke="#fff" stroke-width="2"/>' +
    '</svg>'
  )
}

function createDroneMarker(d, color, selected) {
  const el = document.createElement('div')
  el.className = 'drone-marker'
  el.style.cssText =
    'width:44px;height:44px;cursor:pointer;user-select:none;filter:drop-shadow(0 2px 4px rgba(0,0,0,.45))'
  el.innerHTML = droneSvg(color, selected)
  el.title = d.name || d.drone_tag
  el.addEventListener('click', (ev) => {
    ev.stopPropagation()
    emit('select', d.drone_tag)
  })
  const marker = new Marker({ element: el, anchor: 'center', rotationAlignment: 'map' })
  marker.setLngLat([d.last_telemetry.lon, d.last_telemetry.lat]).addTo(map)
  return { marker, el, key: `${color}|${selected}` }
}

function syncDrones() {
  if (!map) return
  const seen = new Set()
  for (const d of props.drones || []) {
    const t = d.last_telemetry || {}
    const lat = Number(t.lat || d.home_lat)
    const lon = Number(t.lon || d.home_lon)
    if (!Number.isFinite(lat) || !Number.isFinite(lon) || (!lat && !lon)) continue
    seen.add(d.drone_tag)
    const color = statusColor(d, t)
    const selected = d.drone_tag === props.selectedId
    const key = `${color}|${selected}`
    let entry = droneMarkers.get(d.drone_tag)
    if (!entry) {
      entry = createDroneMarker({ ...d, last_telemetry: { lat, lon } }, color, selected)
      droneMarkers.set(d.drone_tag, entry)
    } else {
      if (entry.key !== key) {
        entry.el.innerHTML = droneSvg(color, selected)
        entry.key = key
      }
      entry.marker.setLngLat([lon, lat])
    }
    entry.marker.setRotation(((Number(t.yaw) || 0) * 180) / Math.PI)
    entry.el.style.zIndex = selected ? '5' : '1'
  }
  for (const [id, entry] of droneMarkers.entries()) {
    if (!seen.has(id)) {
      entry.marker.remove()
      droneMarkers.delete(id)
    }
  }
}

function flyTo(lon, lat, zoom = 14) {
  if (!map) return
  map.flyTo({ center: [Number(lon), Number(lat)], zoom, duration: 600 })
}

function selectedTelemetry() {
  const d = props.drones.find((x) => x.drone_tag === props.selectedId)
  const t = d && (d.last_telemetry || {})
  return t && Number.isFinite(Number(t.lat)) && Number.isFinite(Number(t.lon)) ? t : null
}

watch(() => props.fences, syncFences, { deep: true })
watch(
  () => props.route,
  () => {
    syncRoute()
    if (props.autoFitRoute) fitRoute()
  },
  { deep: true }
)
watch(() => props.draftPoints, syncDraft, { deep: true })
watch(() => props.drones, syncDrones, { deep: true })
watch(
  () => props.track,
  () => {
    syncTrack(true)
    fitTrack()
  },
  { deep: true }
)
watch(() => props.trackProgress, () => syncTrack())
watch(
  () => props.selectedId,
  () => {
    syncDrones()
    const t = selectedTelemetry()
    if (t) flyTo(t.lon, t.lat, Math.max(map?.getZoom() || 0, 14))
  }
)
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
watch(
  () => props.drawMode,
  (mode) => {
    if (map) map.getCanvas().style.cursor = mode === 'none' ? '' : 'crosshair'
  }
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
    addOverlayLayers()
    syncDrones()
    const t = selectedTelemetry()
    if (t) flyTo(t.lon, t.lat, 14)
    syncTrack(true)
    if (cachedTrack.length > 0) fitTrack()
    if (props.autoFitRoute && props.route.length > 0) fitRoute()
    map.getCanvas().style.cursor = props.drawMode === 'none' ? '' : 'crosshair'
    emit('ready', map)
  })

  map.on('click', (e) => {
    if (props.drawMode === 'none') return
    emit('map-click', { lon: e.lngLat.lng, lat: e.lngLat.lat })
  })
})

onBeforeUnmount(() => {
  for (const entry of droneMarkers.values()) entry.marker.remove()
  droneMarkers.clear()
  if (trackStartMarker) trackStartMarker.remove()
  if (trackEndMarker) trackEndMarker.remove()
  trackStartMarker = null
  trackEndMarker = null
  cachedTrack = []
  if (map) map.remove()
  map = null
})

defineExpose({ flyTo, fitTrack, fitRoute })
</script>

<style scoped>
.drone-map2d {
  width: 100%;
  height: 100%;
  min-height: 320px;
  background: #0f172a;
}
</style>
