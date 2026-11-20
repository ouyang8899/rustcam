//

export const SRS_WEB_MERCATOR = 'EPSG:3857'
export const SRS_WGS84 = 'EPSG:4326'
export const SRS_CGCS2000 = 'EPSG:4490'

export const SUPPORTED_2D_SRS = [SRS_WEB_MERCATOR]

export const PROTOCOL_XYZ = 'xyz'
export const PROTOCOL_TMS = 'tms'
export const PROTOCOL_WMTS = 'wmts'
export const PROTOCOL_QUADKEY = 'quadkey'

export const TILE_PROTOCOLS = [
  {
    value: PROTOCOL_XYZ,
    label: 'XYZ（行号自上而下）',
    placeholder: 'https://host/{z}/{x}/{y}.png',
    note: '标准瓦片网格，占位符 {x}/{y}/{z}',
  },
  {
    value: PROTOCOL_TMS,
    label: 'TMS（行号自下而上）',
    placeholder: 'https://host/{z}/{x}/{-y}.png',
    note: '行号需要翻转；地址里写 {-y} 会被换成 {y} 并由引擎翻转',
  },
  {
    value: PROTOCOL_WMTS,
    label: 'WMTS（TILEMATRIX/TILEROW/TILECOL）',
    placeholder:
      'https://host/ows?TILEMATRIX={z}&TILEROW={y}&TILECOL={x}',
    note: '天地图等 WMTS 服务的 GetTile 地址',
  },
  {
    value: PROTOCOL_QUADKEY,
    label: 'Quadkey（Bing）',
    placeholder: 'https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1',
    note: 'Bing 风格，只有 2D（MapLibre）能用',
  },
]

const PROTOCOL_FALLBACK = TILE_PROTOCOLS[0]

export function detectProtocol(url) {
  const lower = String(url || '').toLowerCase()
  if (lower.includes('{quadkey}')) return PROTOCOL_QUADKEY
  if (lower.includes('tilematrix') && lower.includes('tilerow') && lower.includes('tilecol')) {
    return PROTOCOL_WMTS
  }
  if (lower.includes('{-y}')) return PROTOCOL_TMS
  return PROTOCOL_XYZ
}

export function protocolInfo(protocol) {
  const value = String(protocol || '').trim().toLowerCase()
  return TILE_PROTOCOLS.find((p) => p.value === value) || PROTOCOL_FALLBACK
}

export function protocolMatchesUrl(url, protocol) {
  const lower = String(url || '').trim().toLowerCase()
  if (!lower) return false
  switch (protocolInfo(protocol).value) {
    case PROTOCOL_QUADKEY:
      return lower.includes('{quadkey}')
    case PROTOCOL_WMTS:
      return lower.includes('tilematrix') && lower.includes('tilerow') && lower.includes('tilecol')
    default:
      return (
        lower.includes('{x}') &&
        lower.includes('{z}') &&
        (lower.includes('{y}') || lower.includes('{-y}'))
      )
  }
}

export function protocolFlipsY(protocol) {
  return protocolInfo(protocol).value === PROTOCOL_TMS
}

export function supportsStandardGrid(layer) {
  return protocolInfo(layer && layer.protocol).value !== PROTOCOL_QUADKEY
}

export function detectFormat(url) {
  const path = String(url || '').split('?')[0].toLowerCase()
  if (path.endsWith('.png')) return 'png'
  if (path.endsWith('.jpg') || path.endsWith('.jpeg')) return 'jpeg'
  if (path.endsWith('.webp')) return 'webp'
  return ''
}

export function tileFormat(layer) {
  const format = String((layer && layer.format) || '').trim().toLowerCase()
  if (format) return format
  return detectFormat(layer && layer.url)
}

export function tileSize(layer) {
  const size = Number(layer && layer.tile_size)
  if (size === 512) return 512
  return 256
}

export function normalizeSrs(srs) {
  const s = String(srs || '').trim()
  const m = /^epsg:\s*(\d+)$/i.exec(s)
  return m ? `EPSG:${m[1]}` : s
}

export function clampZoom(value, fallback) {
  const n = Number(value)
  if (!Number.isFinite(n)) return fallback
  return Math.min(24, Math.max(0, Math.round(n)))
}

export function selectLayers(layers, dim) {
  return (Array.isArray(layers) ? layers : []).filter(
    (l) => l && l.dim === dim && l.enabled !== false,
  )
}

export function buildRasterSource(layer) {
  if (!layer) return null
  if (!SUPPORTED_2D_SRS.includes(normalizeSrs(layer.srs))) return null

  const protocol = String(layer.protocol || '').trim().toLowerCase() || detectProtocol(layer.url)
  const urls = String(layer.url || '')
    .split('\n')
    .map((u) => u.trim())
    .filter((u) => u.length > 0 && protocolMatchesUrl(u, protocol))
  if (urls.length === 0) return null

  const hasNegY = urls.some((u) => u.includes('{-y}'))
  const tiles = hasNegY ? urls.map((u) => u.replace(/\{-y\}/g, '{y}')) : urls
  const scheme = hasNegY || protocolFlipsY(protocol) ? 'tms' : 'xyz'

  return {
    type: 'raster',
    tiles,
    tileSize: tileSize(layer),
    minzoom: clampZoom(layer.min_zoom, 0),
    maxzoom: clampZoom(layer.max_zoom, 19),
    scheme,
  }
}

export function viewZoomRange(layer) {
  if (!layer) return { minZoom: 0, maxZoom: 19 }
  const minZoom = clampZoom(layer.min_zoom, 0)
  const maxZoom = clampZoom(layer.max_zoom, 19)
  return maxZoom >= minZoom ? { minZoom, maxZoom } : { minZoom: 0, maxZoom: 19 }
}

export function pickTilingScheme(srs) {
  switch (normalizeSrs(srs)) {
    case SRS_WEB_MERCATOR:
      return 'webmercator'
    case SRS_WGS84:
    case SRS_CGCS2000:
      return 'geographic'
    default:
      return null
  }
}

export function buildWfsUrl(baseUrl, srs) {
  const base = String(baseUrl || '').trim()
  if (!base) return ''
  const s = String(srs || '').trim()
  if (!s) return base
  const sep = base.includes('?') ? '&' : '?'
  return `${base}${sep}srsName=${encodeURIComponent(s)}`
}
