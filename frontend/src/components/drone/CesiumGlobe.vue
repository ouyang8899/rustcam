<!-- @ts-check -->
<template>
  <div
    ref="container"
    class="cesium-globe"
    :class="{ 'is-drawing': drawMode !== 'none' }"
  >
    <div v-if="!ready" class="globe-loading">
      <el-icon class="is-loading"><Loading /></el-icon>
      <span>正在加载三维地球…</span>
    </div>
    <div class="globe-hint">
      <slot name="hint" />
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount, watch, shallowRef } from 'vue'
import * as Cesium from 'cesium'
import 'cesium/Build/Cesium/Widgets/widgets.css'
import { Loading } from '@element-plus/icons-vue'
import { droneTagFromEntityId } from '../../utils/droneVideo.js'
import { fenceColors } from '../../utils/fenceStyle'
import { pickTilingScheme, protocolFlipsY, selectLayers, supportsStandardGrid } from '../../utils/mapTiles'
import { useSettingsStore } from '../../stores/settingsStore'
import { useToast } from '../../composables/useToast'

const toast = useToast()

const settingsStore = useSettingsStore()

const DRONE_MODEL_URI = import.meta.env.VITE_DRONE_MODEL || '/models/drone.glb'

const props = defineProps({
  drones: { type: Array, default: () => [] },
  selectedId: { type: String, default: '' },
  route: { type: Array, default: () => [] },
  autoFitRoute: { type: Boolean, default: false },
  track: { type: Array, default: () => [] },
  fences: { type: Array, default: () => [] },
  draftPoints: { type: Array, default: () => [] },
  draftCircle: { type: Object, default: null },
  drawMode: { type: String, default: 'none' },
  draftRule: { type: String, default: 'no_fly' },
  drawingActive: { type: Boolean, default: false },
  center: { type: Object, default: () => ({ lon: 121.4737, lat: 31.2304, alt: 1800000 }) },
})

const emit = defineEmits(['map-click', 'ready', 'select', 'finish'])

const container = ref(null)
const ready = ref(false)
const viewer = shallowRef(null)
let clickHandler = null
let moveHandler = null
let hoverPosition = null
let draftRaf = 0
let droneEntities = new Map()
let routeEntities = []
let trackEntities = []
let fenceEntities = []
let draftEntities = []

function setEntity(entity) {
  return entity
}

function pickLngLat(position) {
  const v = viewer.value
  if (!v) return null
  const cartesian = v.camera.pickEllipsoid(position, v.scene.globe.ellipsoid)
  if (!cartesian) return null
  const cartographic = Cesium.Cartographic.fromCartesian(cartesian)
  return {
    lon: Cesium.Math.toDegrees(cartographic.longitude),
    lat: Cesium.Math.toDegrees(cartographic.latitude),
  }
}

function distanceMeters(a, b) {
  const R = 6371000
  const dLat = ((b.lat - a.lat) * Math.PI) / 180
  const dLon = ((b.lon - a.lon) * Math.PI) / 180
  const lat1 = (a.lat * Math.PI) / 180
  const lat2 = (b.lat * Math.PI) / 180
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(lat1) * Math.cos(lat2) * Math.sin(dLon / 2) ** 2
  return 2 * R * Math.asin(Math.sqrt(h))
}

function scheduleDraft() {
  if (draftRaf) return
  draftRaf = requestAnimationFrame(() => {
    draftRaf = 0
    syncDraft()
  })
}

function onCanvasContextMenu(e) {
  if (props.drawMode === 'polygon' || props.drawMode === 'circle') e.preventDefault()
}

function initViewer() {
  const v = new Cesium.Viewer(container.value, {
    baseLayer: false,
    animation: false,
    timeline: false,
    baseLayerPicker: false,
    geocoder: false,
    homeButton: false,
    sceneModePicker: false,
    navigationHelpButton: false,
    fullscreenButton: false,
    infoBox: false,
    selectionIndicator: false,
    terrainProvider: new Cesium.EllipsoidTerrainProvider(),
    requestRenderMode: false,
  })
  const creditContainer = v.cesiumWidget?.creditContainer
  if (creditContainer) creditContainer.style.display = 'none'
  const customTms = import.meta.env.VITE_CESIUM_TMS
  const layers = selectLayers(settingsStore.browserLayers, '3d')
  let added = false

  for (const layer of layers) {
    const scheme = pickTilingScheme(layer.srs)
    if (!scheme) {
      console.warn('[CesiumGlobe] 3D 图层投影不受支持，已跳过:', layer.name || layer.url, layer.srs)
      toast.warning(
        `3D 图层「${layer.name || layer.url}」的投影 ${layer.srs} 不被浏览器 3D 支持（仅 EPSG:3857 / 4326 / 4490），已跳过`,
      )
      continue
    }
    if (!supportsStandardGrid(layer)) {
      const bingKey = String(layer.bing_key || '').trim()
      if (!bingKey) {
        console.warn('[CesiumGlobe] 3D 图层缺少 Bing Key，已跳过:', layer.name || layer.url, layer.protocol)
        toast.warning(
          `3D 图层「${layer.name || layer.url}」使用 ${layer.protocol} 协议，需要配置 Bing Maps API Key 才能在 3D 中显示，已跳过`,
        )
        continue
      }
      const mapStyle = String(layer.imagery_set || 'Aerial').toLowerCase() === 'road'
        ? Cesium.BingMapsStyle.ROAD
        : Cesium.BingMapsStyle.AERIAL
      Cesium.BingMapsImageryProvider
        .fromUrl('https://dev.virtualearth.net', { key: bingKey, mapStyle })
        .then((provider) => {
          v.imageryLayers.addImageryProvider(provider)
        })
        .catch((err) => {
          console.error('[CesiumGlobe] Bing 影像加载失败:', err)
          toast.error(`3D 图层「${layer.name || layer.url}」的 Bing 影像加载失败：${err?.message || err}`)
        })
      added = true
      break
    }
    const tilingScheme = scheme === 'webmercator'
      ? new Cesium.WebMercatorTilingScheme()
      : new Cesium.GeographicTilingScheme()
    let url = String(layer.url || '')
    if (protocolFlipsY(layer.protocol)) {
      url = url.replace(/\{-y\}/g, '{reverseY}').replace(/\{y\}/g, '{reverseY}')
    } else {
      url = url.replace(/\{-y\}/g, '{y}')
    }
    v.imageryLayers.addImageryProvider(
      new Cesium.UrlTemplateImageryProvider({
        url,
        tilingScheme,
        minimumLevel: Math.max(0, Number(layer.min_zoom) || 0),
        maximumLevel: Math.max(0, Number(layer.max_zoom) || 19),
      })
    )
    added = true
    break
  }

  if (!added) {
    const base = (window.CESIUM_BASE_URL || '/cesium/').replace(/\/?$/, '/')
    if (customTms) {
      v.imageryLayers.addImageryProvider(
        new Cesium.UrlTemplateImageryProvider({
          url: customTms.replace(/\{-y\}/g, '{reverseY}'),
          maximumLevel: 20,
        })
      )
    } else if (import.meta.env.VITE_CESIUM_ONLINE === 'true') {
      v.imageryLayers.addImageryProvider(
        new Cesium.OpenStreetMapImageryProvider({ url: 'https://tile.openstreetmap.org/' })
      )
    } else {
      v.imageryLayers.addImageryProvider(
        new Cesium.UrlTemplateImageryProvider({
          url: `${base}Assets/Textures/NaturalEarthII/{z}/{x}/{reverseY}.jpg`,
          tilingScheme: new Cesium.GeographicTilingScheme(),
          maximumLevel: 2,
          credit: 'Natural Earth II',
        })
      )
    }
  }
  v.scene.globe.baseColor = Cesium.Color.fromCssColorString('#0b1b2b')
  v.scene.globe.depthTestAgainstTerrain = false
  v.scene.globe.enableLighting = false
  v.scene.skyAtmosphere.show = true
  v.scene.fog.enabled = true
  v.camera.setView({
    destination: Cesium.Cartesian3.fromDegrees(
      props.center.lon,
      props.center.lat,
      props.center.alt || 1800000
    ),
    orientation: { heading: 0, pitch: Cesium.Math.toRadians(-90), roll: 0 },
  })

  clickHandler = new Cesium.ScreenSpaceEventHandler(v.scene.canvas)
  clickHandler.setInputAction((movement) => {
    const picked = v.scene.pick(movement.position)
    const droneTag = droneTagFromEntityId(picked?.id?.id)
    if (droneTag) {
      emit('select', droneTag)
      return
    }
    if (props.drawMode === 'none') return
    const p = pickLngLat(movement.position)
    if (p) emit('map-click', p)
  }, Cesium.ScreenSpaceEventType.LEFT_CLICK)

  clickHandler.setInputAction((movement) => {
    if (!props.drawingActive) return
    if (props.drawMode !== 'polygon' && props.drawMode !== 'circle') return
    emit('finish', pickLngLat(movement.position) || hoverPosition)
  }, Cesium.ScreenSpaceEventType.RIGHT_CLICK)

  moveHandler = new Cesium.ScreenSpaceEventHandler(v.scene.canvas)
  moveHandler.setInputAction((movement) => {
    if (!props.drawingActive || (props.drawMode !== 'polygon' && props.drawMode !== 'circle')) {
      hoverPosition = null
      return
    }
    hoverPosition = pickLngLat(movement.endPosition)
    scheduleDraft()
  }, Cesium.ScreenSpaceEventType.MOUSE_MOVE)
  v.scene.canvas.addEventListener('contextmenu', onCanvasContextMenu)

  viewer.value = v
  ready.value = true
  syncDrones()
  syncRoute()
  syncTrack()
  syncFences()
  syncDraft()
  if (props.autoFitRoute && props.route.length > 0) fitRoute()
  emit('ready', v)
}

function syncDrones() {
  const v = viewer.value
  if (!v) return
  const seen = new Set()
  for (const d of props.drones) {
    const t = d.last_telemetry || {}
    const lat = t.lat || d.home_lat || props.center.lat
    const lon = t.lon || d.home_lon || props.center.lon
    const alt = t.alt_amsl || 30
    if (!lat || !lon) continue
    seen.add(d.drone_tag)
    const position = Cesium.Cartesian3.fromDegrees(lon, lat, alt)
    let entity = droneEntities.get(d.drone_tag)
    if (!entity) {
      entity = v.entities.add({
        id: `drone-${d.drone_tag}`,
        position,
        model: {
          uri: DRONE_MODEL_URI,
          minimumPixelSize: 64,
          maximumScale: 200,
          colorBlendMode: Cesium.ColorBlendMode.MIX,
          colorBlendAmount: 0,
        },
        label: {
          text: d.name || d.drone_tag,
          font: '12px "Noto Sans SC", sans-serif',
          fillColor: Cesium.Color.WHITE,
          showBackground: true,
          backgroundColor: Cesium.Color.fromCssColorString('rgba(15,23,42,0.75)'),
          pixelOffset: new Cesium.Cartesian2(0, -28),
          verticalOrigin: Cesium.VerticalOrigin.BOTTOM,
        },
      })
      droneEntities.set(d.drone_tag, entity)
    } else {
      entity.position = position
      if (entity.label) entity.label.text = d.name || d.drone_tag
    }
    const heading = t.yaw !== undefined ? Number(t.yaw) : 0
    const pitch = t.pitch !== undefined ? Number(t.pitch) : 0
    const roll = t.roll !== undefined ? Number(t.roll) : 0
    const hpr = new Cesium.HeadingPitchRoll(heading, pitch, roll)
    entity.orientation = Cesium.Transforms.headingPitchRollQuaternion(position, hpr)
    const color = !d.online
      ? Cesium.Color.fromCssColorString('#94a3b8')
      : t.armed
        ? Cesium.Color.fromCssColorString('#f97316')
        : Cesium.Color.fromCssColorString('#22c55e')
    if (entity.model) {
      entity.model.color = color
      entity.model.colorBlendMode = Cesium.ColorBlendMode.MIX
      entity.model.colorBlendAmount = 0.35
    }
    entity.show = true
  }
  for (const [id, entity] of droneEntities.entries()) {
    if (!seen.has(id)) {
      v.entities.remove(entity)
      droneEntities.delete(id)
    }
  }
}

function clearEntities(list) {
  const v = viewer.value
  if (!v) return
  for (const e of list) v.entities.remove(e)
  return []
}

function syncRoute() {
  const v = viewer.value
  if (!v) return
  routeEntities = clearEntities(routeEntities) || []
  const points = props.route || []
  if (points.length === 0) return
  const positions = points.map((w) =>
    Cesium.Cartesian3.fromDegrees(w.lon, w.lat, w.alt + 0.5)
  )
  routeEntities.push(
    v.entities.add({
      polyline: {
        positions,
        width: 4,
        material: Cesium.Color.fromCssColorString('#38bdf8'),
        clampToGround: false,
      },
    })
  )
  points.forEach((w, i) => {
    routeEntities.push(
      v.entities.add({
        position: positions[i],
        point: {
          pixelSize: 10,
          color: Cesium.Color.fromCssColorString('#0ea5e9'),
          outlineColor: Cesium.Color.WHITE,
          outlineWidth: 2,
        },
        label: {
          text: `WP${i + 1} ${Math.round(w.alt)}m`,
          font: '11px "JetBrains Mono", monospace',
          fillColor: Cesium.Color.WHITE,
          showBackground: true,
          backgroundColor: Cesium.Color.fromCssColorString('rgba(2,132,199,0.8)'),
          pixelOffset: new Cesium.Cartesian2(0, -22),
          verticalOrigin: Cesium.VerticalOrigin.BOTTOM,
        },
      })
    )
  })
}

function syncTrack() {
  const v = viewer.value
  if (!v) return
  trackEntities = clearEntities(trackEntities) || []
  const points = props.track || []
  if (points.length === 0) return
  const valid = points
    .map((p) => ({
      lon: Number(p.lon),
      lat: Number(p.lat),
      alt: Number(p.alt ?? p.alt_amsl ?? 30),
    }))
    .filter((p) => Number.isFinite(p.lon) && Number.isFinite(p.lat))
  if (valid.length === 0) return
  const positions = valid.map((p) =>
    Cesium.Cartesian3.fromDegrees(p.lon, p.lat, (Number.isFinite(p.alt) ? p.alt : 30) + 0.5)
  )
  if (positions.length >= 2) {
    trackEntities.push(
      v.entities.add({
        polyline: {
          positions,
          width: 4,
          material: Cesium.Color.fromCssColorString('#00D4FF'),
          clampToGround: false,
        },
      })
    )
  }
  const ends = [{ point: valid[0], text: '起点', color: '#22c55e' }]
  if (valid.length > 1) {
    ends.push({ point: valid[valid.length - 1], text: '终点', color: '#ef4444' })
  }
  for (const end of ends) {
    trackEntities.push(
      v.entities.add({
        position: Cesium.Cartesian3.fromDegrees(
          end.point.lon,
          end.point.lat,
          (Number.isFinite(end.point.alt) ? end.point.alt : 30) + 0.5
        ),
        point: {
          pixelSize: 8,
          color: Cesium.Color.fromCssColorString(end.color),
          outlineColor: Cesium.Color.WHITE,
          outlineWidth: 2,
        },
        label: {
          text: end.text,
          font: '11px "Noto Sans SC", sans-serif',
          fillColor: Cesium.Color.WHITE,
          showBackground: true,
          backgroundColor: Cesium.Color.fromCssColorString('rgba(15,23,42,0.75)'),
          pixelOffset: new Cesium.Cartesian2(0, -18),
          verticalOrigin: Cesium.VerticalOrigin.BOTTOM,
        },
      })
    )
  }
}

function geojsonRings(geojson) {
  try {
    const value = typeof geojson === 'string' ? JSON.parse(geojson) : geojson
    if (!value) return []
    if (value.type === 'Polygon') return [value.coordinates[0]]
    if (value.type === 'MultiPolygon') return value.coordinates.map((p) => p[0])
    if (value.type === 'Feature') return geojsonRings(value.geometry)
    if (value.type === 'FeatureCollection') {
      return (value.features || []).flatMap((f) => geojsonRings(f))
    }
    return []
  } catch (e) {
    return []
  }
}

function syncFences() {
  const v = viewer.value
  if (!v) return
  fenceEntities = clearEntities(fenceEntities) || []
  for (const f of props.fences || []) {
    const colors = fenceColors(f.type)
    const color = Cesium.Color.fromCssColorString(colors.fillRgba)
    const outline = Cesium.Color.fromCssColorString(colors.outline)
    if (f.shape === 'circle' && f.radius_m > 0) {
      fenceEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(f.center_lon, f.center_lat, 0),
          ellipse: {
            semiMajorAxis: f.radius_m,
            semiMinorAxis: f.radius_m,
            height: f.min_alt || 0,
            extrudedHeight: f.max_alt || 120,
            material: color,
            outline: true,
            outlineColor: outline,
          },
          label: {
            text: f.name,
            font: '12px "Noto Sans SC", sans-serif',
            fillColor: outline,
            pixelOffset: new Cesium.Cartesian2(0, -10),
          },
        })
      )
      continue
    }
    for (const ring of geojsonRings(f.geojson)) {
      if (!ring || ring.length < 3) continue
      const flat = []
      for (const p of ring) {
        flat.push(p[0], p[1])
      }
      fenceEntities.push(
        v.entities.add({
          polygon: {
            hierarchy: Cesium.Cartesian3.fromDegreesArray(flat),
            height: f.min_alt || 0,
            extrudedHeight: f.max_alt || 120,
            material: color,
            outline: true,
            outlineColor: outline,
          },
        })
      )
      fenceEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(
            ring[0][0],
            ring[0][1],
            (f.max_alt || 120) + 10
          ),
          label: {
            text: f.name,
            font: '12px "Noto Sans SC", sans-serif',
            fillColor: outline,
            showBackground: true,
            backgroundColor: Cesium.Color.fromCssColorString('rgba(15,23,42,0.7)'),
            pixelOffset: new Cesium.Cartesian2(0, -10),
          },
        })
      )
    }
  }
}

function syncDraft() {
  const v = viewer.value
  if (!v) return
  draftEntities = clearEntities(draftEntities) || []
  const points = props.draftPoints || []
  if (props.drawMode === 'waypoint' && points.length > 0) {
    const positions = points.map((p) =>
      Cesium.Cartesian3.fromDegrees(p.lon, p.lat, (p.alt || 30) + 0.5)
    )
    draftEntities.push(
      v.entities.add({
        polyline: {
          positions,
          width: 3,
          material: new Cesium.PolylineDashMaterialProperty({
            color: Cesium.Color.fromCssColorString('#fbbf24'),
          }),
        },
      })
    )
    points.forEach((p, i) => {
      draftEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(p.lon, p.lat, (p.alt || 30) + 0.5),
          point: {
            pixelSize: 9,
            color: Cesium.Color.fromCssColorString('#fbbf24'),
            outlineColor: Cesium.Color.WHITE,
            outlineWidth: 2,
          },
          label: {
            text: `${i + 1}`,
            font: '11px "JetBrains Mono", monospace',
            fillColor: Cesium.Color.BLACK,
            showBackground: true,
            backgroundColor: Cesium.Color.fromCssColorString('#fbbf24'),
            pixelOffset: new Cesium.Cartesian2(0, -18),
          },
        })
      )
    })
  }
  if (props.drawMode === 'polygon' && points.length > 0) {
    const preview = hoverPosition ? [...points, hoverPosition] : points
    const draft = fenceColors(props.draftRule)
    if (preview.length >= 3) {
      const flat = []
      for (const p of preview) flat.push(p.lon, p.lat)
      draftEntities.push(
        v.entities.add({
          polygon: {
            hierarchy: Cesium.Cartesian3.fromDegreesArray(flat),
            height: 0,
            material: Cesium.Color.fromCssColorString(draft.fillRgba),
            outline: true,
            outlineColor: Cesium.Color.fromCssColorString(draft.outline),
          },
        })
      )
    }
    if (hoverPosition && preview.length >= 2) {
      const path = points.length >= 2 ? [...preview, points[0]] : preview
      draftEntities.push(
        v.entities.add({
          polyline: {
            positions: path.map((p) => Cesium.Cartesian3.fromDegrees(p.lon, p.lat, 1)),
            width: 2,
            material: new Cesium.PolylineDashMaterialProperty({
              color: Cesium.Color.fromCssColorString(draft.outline),
            }),
          },
        })
      )
    }
    points.forEach((p) => {
      draftEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(p.lon, p.lat, 1),
          point: {
            pixelSize: 8,
            color: Cesium.Color.fromCssColorString(draft.outline),
            outlineColor: Cesium.Color.WHITE,
            outlineWidth: 2,
          },
        })
      )
    })
    if (hoverPosition) {
      draftEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(hoverPosition.lon, hoverPosition.lat, 1),
          point: {
            pixelSize: 7,
            color: Cesium.Color.fromCssColorString('#fbbf24'),
            outlineColor: Cesium.Color.WHITE,
            outlineWidth: 2,
          },
        })
      )
    }
  }
  if (props.drawMode === 'circle' && props.draftCircle) {
    const c = props.draftCircle
    const draft = fenceColors(props.draftRule)
    const fixedRadius = Number(c.radius_m) > 0
    const radius =
      fixedRadius || !hoverPosition
        ? Number(c.radius_m)
        : distanceMeters({ lat: c.lat, lon: c.lon }, hoverPosition)
    if (radius > 0) {
      draftEntities.push(
        v.entities.add({
          position: Cesium.Cartesian3.fromDegrees(c.lon, c.lat, 0),
          ellipse: {
            semiMajorAxis: radius,
            semiMinorAxis: radius,
            height: 0,
            extrudedHeight: 80,
            material: Cesium.Color.fromCssColorString(draft.fillRgba),
            outline: true,
            outlineColor: Cesium.Color.fromCssColorString(draft.outline),
          },
        })
      )
    }
    draftEntities.push(
      v.entities.add({
        position: Cesium.Cartesian3.fromDegrees(c.lon, c.lat, 1),
        point: {
          pixelSize: 8,
          color: Cesium.Color.fromCssColorString(draft.outline),
          outlineColor: Cesium.Color.WHITE,
          outlineWidth: 2,
        },
      })
    )
  }
}

function flyTo(lon, lat, alt = 600000) {
  const v = viewer.value
  if (!v) return
  v.camera.flyTo({
    destination: Cesium.Cartesian3.fromDegrees(lon, lat, alt),
    orientation: { heading: 0, pitch: Cesium.Math.toRadians(-70), roll: 0 },
    duration: 1.5,
  })
}

function fitRoute() {
  if (!viewer.value || props.route.length === 0) return
  const points = props.route.map((p) => ({ lon: Number(p.lon), lat: Number(p.lat) }))
  const d = props.drones.find((x) => x.drone_tag === props.selectedId)
  const t = d?.last_telemetry
  if (t && Number.isFinite(Number(t.lon)) && Number.isFinite(Number(t.lat))) {
    points.push({ lon: Number(t.lon), lat: Number(t.lat) })
  }
  const valid = points.filter((p) => Number.isFinite(p.lon) && Number.isFinite(p.lat))
  if (valid.length === 0) return
  let minLat = 90
  let maxLat = -90
  let minLon = 180
  let maxLon = -180
  for (const p of valid) {
    if (p.lat < minLat) minLat = p.lat
    if (p.lat > maxLat) maxLat = p.lat
    if (p.lon < minLon) minLon = p.lon
    if (p.lon > maxLon) maxLon = p.lon
  }
  const midLat = (minLat + maxLat) / 2
  const span = Math.max(
    (maxLat - minLat) * 111320,
    (maxLon - minLon) * 111320 * Math.cos((midLat * Math.PI) / 180),
    300
  )
  flyTo((minLon + maxLon) / 2, midLat, Math.min(2_000_000, Math.max(1500, span * 3)))
}

watch(() => props.drones, syncDrones, { deep: true })
watch(
  () => props.route,
  () => {
    syncRoute()
    if (props.autoFitRoute) fitRoute()
  },
  { deep: true }
)
watch(() => props.track, syncTrack, { deep: true })
watch(() => props.fences, syncFences, { deep: true })
watch(() => props.draftPoints, syncDraft, { deep: true })
watch(() => props.drawMode, () => {
  hoverPosition = null
  syncDraft()
})
watch(() => props.drawingActive, () => {
  hoverPosition = null
  syncDraft()
})
watch(() => props.draftCircle, syncDraft, { deep: true })
watch(() => props.selectedId, (id) => {
  const drone = props.drones.find((d) => d.drone_tag === id)
  const t = drone?.last_telemetry
  if (t && t.lat && t.lon) flyTo(t.lon, t.lat, 600000)
})

onMounted(async () => {
  await settingsStore.loadMapSettings()
  initViewer()
})
onBeforeUnmount(() => {
  if (clickHandler) clickHandler.destroy()
  if (moveHandler) moveHandler.destroy()
  if (draftRaf) cancelAnimationFrame(draftRaf)
  if (viewer.value) viewer.value.scene.canvas.removeEventListener('contextmenu', onCanvasContextMenu)
  if (viewer.value) viewer.value.destroy()
  viewer.value = null
})

defineExpose({ flyTo, viewer, fitRoute })
</script>

<style scoped>
.cesium-globe {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 320px;
  overflow: hidden;
  background: #0b1220;
}
.cesium-globe :deep(.cesium-viewer),
.cesium-globe :deep(.cesium-viewer-cesiumWidgetContainer),
.cesium-globe :deep(.cesium-widget),
.cesium-globe :deep(canvas) {
  width: 100%;
  height: 100%;
}
.cesium-globe.is-drawing :deep(canvas) {
  cursor: crosshair !important;
}
.cesium-globe :deep(.cesium-widget-credits),
.cesium-globe :deep(.cesium-credit-logoContainer),
.cesium-globe :deep(.cesium-credit-textContainer) {
  display: none !important;
}
.globe-loading {
  position: absolute;
  inset: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: #93c5fd;
  background: #0b1220;
  font-size: 13px;
}
.globe-hint {
  position: absolute;
  left: 12px;
  bottom: 12px;
  z-index: 10;
  pointer-events: none;
}
</style>
