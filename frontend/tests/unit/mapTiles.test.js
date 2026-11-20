// @ts-check
import { test } from 'node:test'
import assert from 'node:assert/strict'

import {
  buildRasterSource,
  buildWfsUrl,
  clampZoom,
  detectFormat,
  detectProtocol,
  normalizeSrs,
  pickTilingScheme,
  protocolFlipsY,
  protocolMatchesUrl,
  selectLayers,
  supportsStandardGrid,
  tileFormat,
  tileSize,
  viewZoomRange,
} from '../../src/utils/mapTiles.js'

test('SRS 归一化统一 EPSG 前缀', () => {
  assert.equal(normalizeSrs('epsg:3857'), 'EPSG:3857')
  assert.equal(normalizeSrs('  EPSG:4326  '), 'EPSG:4326')
  assert.equal(normalizeSrs('+proj=longlat +datum=WGS84'), '+proj=longlat +datum=WGS84')
  assert.equal(normalizeSrs(''), '')
})

test('缩放层级夹到 0..24', () => {
  assert.equal(clampZoom(19, 0), 19)
  assert.equal(clampZoom(30, 0), 24)
  assert.equal(clampZoom(-3, 0), 0)
  assert.equal(clampZoom('abc', 7), 7)
  assert.equal(clampZoom(undefined, 5), 5)
})

test('按维度筛选启用图层', () => {
  const layers = [
    { dim: '2d', enabled: true, url: 'a' },
    { dim: '3d', enabled: true, url: 'b' },
    { dim: '2d', enabled: false, url: 'c' },
    { dim: '2d', url: 'd' },
  ]
  assert.deepEqual(selectLayers(layers, '2d').map((l) => l.url), ['a', 'd'])
  assert.deepEqual(selectLayers(layers, '3d').map((l) => l.url), ['b'])
  assert.deepEqual(selectLayers(null, '2d'), [])
})

test('协议从地址结构自动推断', () => {
  assert.equal(detectProtocol('https://h/{z}/{x}/{y}.png'), 'xyz')
  assert.equal(detectProtocol('https://h/{z}/{x}/{-y}.png'), 'tms')
  assert.equal(detectProtocol('https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1'), 'quadkey')
  assert.equal(
    detectProtocol('https://h/ows?TILEMATRIX={z}&TILEROW={y}&TILECOL={x}'),
    'wmts',
  )
})

test('协议与地址的占位符约定', () => {
  assert.ok(protocolMatchesUrl('https://h/{z}/{x}/{y}.png', 'xyz'))
  assert.ok(protocolMatchesUrl('https://h/{z}/{x}/{y}.png', 'tms'))
  assert.ok(!protocolMatchesUrl('https://h/{z}/{x}/{y}.png', 'quadkey'))
  assert.ok(protocolMatchesUrl('https://h/a{quadkey}.jpeg', 'quadkey'))
  assert.ok(protocolMatchesUrl('https://h/ows?TILEMATRIX={z}&TILEROW={y}&TILECOL={x}', 'wmts'))
  assert.ok(!protocolMatchesUrl('', 'xyz'))
  assert.equal(protocolFlipsY('tms'), true)
  assert.equal(protocolFlipsY('xyz'), false)
})

test('影像格式与瓦片尺寸', () => {
  assert.equal(detectFormat('https://h/a.png'), 'png')
  assert.equal(detectFormat('https://h/a.jpeg?x=1'), 'jpeg')
  assert.equal(detectFormat('https://h/a.webp'), 'webp')
  assert.equal(detectFormat('https://h/{z}/{x}/{y}'), '')
  assert.equal(tileFormat({ format: 'JPEG' }), 'jpeg')
  assert.equal(tileFormat({ url: 'https://h/a.png' }), 'png')
  assert.equal(tileSize({ tile_size: 512 }), 512)
  assert.equal(tileSize({}), 256)
})

test('Bing quadkey 图层在 2D 可用、3D 不可用', () => {
  const bing = {
    dim: '2d',
    protocol: 'quadkey',
    url: 'https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1',
    srs: 'EPSG:3857',
    min_zoom: 0,
    max_zoom: 19,
  }
  const src = buildRasterSource(bing)
  assert.ok(src)
  assert.deepEqual(src.tiles, ['https://t0.tiles.virtualearth.net/tiles/a{quadkey}.jpeg?g=1'])
  assert.equal(src.scheme, 'xyz')
  assert.equal(src.tileSize, 256)
  assert.equal(supportsStandardGrid(bing), false)
  assert.equal(supportsStandardGrid({ protocol: 'xyz' }), true)
  // 协议与地址不匹配时不可用
  assert.equal(buildRasterSource({ ...bing, url: 'https://h/{z}/{x}/{y}.png' }), null)
})

test('tms 协议的行号交给 MapLibre 翻转', () => {
  const src = buildRasterSource({
    dim: '2d',
    protocol: 'tms',
    url: 'https://h/{z}/{x}/{y}.png',
    srs: 'EPSG:3857',
    min_zoom: 0,
    max_zoom: 19,
  })
  assert.equal(src.scheme, 'tms')
  assert.deepEqual(src.tiles, ['https://h/{z}/{x}/{y}.png'])
})

test('2D 底图只接受 EPSG:3857', () => {
  const base = { dim: '2d', url: 'https://h/{z}/{x}/{y}.png', srs: 'EPSG:3857', min_zoom: 0, max_zoom: 19 }
  assert.ok(buildRasterSource(base))
  assert.equal(buildRasterSource({ ...base, srs: 'EPSG:4326' }), null)
  assert.equal(buildRasterSource({ ...base, srs: 'EPSG:4490' }), null)
  assert.equal(buildRasterSource({ ...base, srs: '' }), null)
})

test('2D 底图按 {-y} 判定 TMS', () => {
  const src = buildRasterSource({
    dim: '2d',
    url: 'https://h/{z}/{x}/{-y}.png',
    srs: 'EPSG:3857',
    min_zoom: 2,
    max_zoom: 18,
  })
  assert.equal(src.scheme, 'tms')
  assert.deepEqual(src.tiles, ['https://h/{z}/{x}/{y}.png'])
  assert.equal(src.minzoom, 2)
  assert.equal(src.maxzoom, 18)
})

test('2D 底图按 XYZ 处理普通 URL', () => {
  const src = buildRasterSource({
    dim: '2d',
    url: 'https://h/{z}/{x}/{y}.png\nhttps://h2/{z}/{x}/{y}.png',
    srs: 'EPSG:3857',
    min_zoom: 0,
    max_zoom: 19,
  })
  assert.equal(src.scheme, 'xyz')
  assert.equal(src.tiles.length, 2)
})

test('2D 底图过滤掉无效 URL', () => {
  assert.equal(
    buildRasterSource({ dim: '2d', url: 'https://h/no-placeholders', srs: 'EPSG:3857' }),
    null,
  )
})

test('3D tiling scheme 选择', () => {
  assert.equal(pickTilingScheme('EPSG:3857'), 'webmercator')
  assert.equal(pickTilingScheme('epsg:3857'), 'webmercator')
  assert.equal(pickTilingScheme('EPSG:4326'), 'geographic')
  assert.equal(pickTilingScheme('EPSG:4490'), 'geographic')
  assert.equal(pickTilingScheme('EPSG:4547'), null)
})

test('WFS URL 追加 srsName', () => {
  assert.equal(
    buildWfsUrl('http://h/ows?service=WFS', 'EPSG:4326'),
    'http://h/ows?service=WFS&srsName=EPSG%3A4326',
  )
  assert.equal(buildWfsUrl('http://h/ows', 'EPSG:4326'), 'http://h/ows?srsName=EPSG%3A4326')
  assert.equal(buildWfsUrl('http://h/ows', ''), 'http://h/ows')
  assert.equal(buildWfsUrl('', 'EPSG:4326'), '')
})

test('视图 zoom 跟随图层范围', () => {
  assert.deepEqual(viewZoomRange({ min_zoom: 3, max_zoom: 17 }), { minZoom: 3, maxZoom: 17 })
  assert.deepEqual(viewZoomRange({ min_zoom: '2', max_zoom: '16' }), { minZoom: 2, maxZoom: 16 })
  assert.deepEqual(viewZoomRange({}), { minZoom: 0, maxZoom: 19 })
  assert.deepEqual(viewZoomRange(null), { minZoom: 0, maxZoom: 19 })
  assert.deepEqual(viewZoomRange({ min_zoom: 12, max_zoom: 5 }), { minZoom: 0, maxZoom: 19 })
})
