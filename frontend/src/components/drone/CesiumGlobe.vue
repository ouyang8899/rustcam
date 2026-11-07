<!-- @ts-check -->
<template>
  <div ref="container" class="cesium-globe">
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

/** 机模地址：默认使用 public/models/drone.glb，可用 VITE_DRONE_MODEL 覆盖。 */
const DRONE_MODEL_URI = import.meta.env.VITE_DRONE_MODEL || '/models/drone.glb'

const props = defineProps({
  drones: { type: Array, default: () => [] },
  selectedId: { type: String, default: '' },
  route: { type: Array, default: () => [] },
  /** 航线变化时自动把相机适配到航线 + 当前无人机（实时飞行页使用） */
  autoFitRoute: { type: Boolean, default: false },
  /** @type {Array<any>} 回放轨迹点 {lat, lon, alt?}：只画折线与起终点，不标航点序号 */
  track: { type: Array, default: () => [] },
  fences: { type: Array, default: () => [] },
  draftPoints: { type: Array, default: () => [] },
  draftCircle: { type: Object, default: null },
  drawMode: { type: String, default: 'none' },
  center: { type: Object, default: () => ({ lon: 121.4737, lat: 31.2304, alt: 1800000 }) },
})

const emit = defineEmits(['map-click', 'ready'])

const container = ref(null)
const ready = ref(false)
const viewer = shallowRef(null)
let clickHandler = null
let droneEntities = new Map()
let routeEntities = []
let trackEntities = []
let fenceEntities = []
let draftEntities = []

function setEntity(entity) {
  return entity
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
  // 离线底图：Cesium 自带 NaturalEarthII，测试/内网环境不依赖外部瓦片服务。
  // 需要在线高清底图时设置 VITE_CESIUM_ONLINE=true。
  const base = (window.CESIUM_BASE_URL || '/cesium/').replace(/\/?$/, '/')
  const customTms = import.meta.env.VITE_CESIUM_TMS
  if (customTms) {
    // 内网/私有化部署：指向 GeoServer / 本地瓦片服务
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
    if (props.drawMode === 'none') return
    const cartesian = v.camera.pickEllipsoid(movement.position, v.scene.globe.ellipsoid)
    if (!cartesian) return
    const cartographic = Cesium.Cartographic.fromCartesian(cartesian)
    emit('map-click', {
      lon: Cesium.Math.toDegrees(cartographic.longitude),
      lat: Cesium.Math.toDegrees(cartographic.latitude),
    })
  }, Cesium.ScreenSpaceEventType.LEFT_CLICK)

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
    // 姿态：遥测里的 yaw/pitch/roll 均为弧度，直接交给 HeadingPitchRoll（规则 §10.6）。
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

/**
 * 回放轨迹：整条折线 + 起终点标记，不逐点标注序号。
 */
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
    const isAllowed = f.type === 'allowed'
    const color = isAllowed
      ? Cesium.Color.fromCssColorString('rgba(34,197,94,0.25)')
      : Cesium.Color.fromCssColorString('rgba(239,68,68,0.25)')
    const outline = isAllowed
      ? Cesium.Color.fromCssColorString('#22c55e')
      : Cesium.Color.fromCssColorString('#ef4444')
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
  if (props.drawMode === 'polygon' && points.length >= 2) {
    const flat = []
    for (const p of points) flat.push(p.lon, p.lat)
    draftEntities.push(
      v.entities.add({
        polygon: {
          hierarchy: Cesium.Cartesian3.fromDegreesArray(flat),
          height: 0,
          material: Cesium.Color.fromCssColorString('rgba(239,68,68,0.2)'),
          outline: true,
          outlineColor: Cesium.Color.fromCssColorString('#ef4444'),
        },
      })
    )
  }
  if (props.drawMode === 'circle' && props.draftCircle) {
    const c = props.draftCircle
    draftEntities.push(
      v.entities.add({
        position: Cesium.Cartesian3.fromDegrees(c.lon, c.lat, 0),
        ellipse: {
          semiMajorAxis: c.radius_m,
          semiMinorAxis: c.radius_m,
          height: 0,
          extrudedHeight: 80,
          material: Cesium.Color.fromCssColorString('rgba(239,68,68,0.2)'),
          outline: true,
          outlineColor: Cesium.Color.fromCssColorString('#ef4444'),
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

/** 把相机适配到航线与当前无人机，保证“意图航线”一眼可见。 */
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
watch(() => props.drawMode, syncDraft)
watch(() => props.draftCircle, syncDraft, { deep: true })
watch(() => props.selectedId, (id) => {
  const drone = props.drones.find((d) => d.drone_tag === id)
  const t = drone?.last_telemetry
  if (t && t.lat && t.lon) flyTo(t.lon, t.lat, 600000)
})

onMounted(initViewer)
onBeforeUnmount(() => {
  if (clickHandler) clickHandler.destroy()
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
