import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const here = dirname(fileURLToPath(import.meta.url))
const siteRoot = resolve(here, '..')
const repoRoot = resolve(siteRoot, '..')
const outDir = join(siteRoot, 'public', 'img')

/** 源文件（相对仓库根） → 输出文件名 */
const MAP = {
  'screenshot/2026-10-29/系统概览.png': 'console-overview.webp',
  'screenshot/2026-10-29/设备管理-设备列表.png': 'console-devices.webp',
  'screenshot/2026-10-29/设备管理-通道列表.png': 'console-channels.webp',
  'screenshot/2026-10-29/设备列表-电子地图2.png': 'console-map-points.webp',
  'screenshot/2026-10-29/监控墙-监控墙.png': 'console-tvwall.webp',
  'screenshot/2026-10-29/录像管理-录像回放.png': 'console-playback.webp',
  'screenshot/2026-10-29-destop/客户端-实时预览.png': 'desktop-live.webp',
  'screenshot/2026-10-29-destop/客户端-电子地图.png': 'desktop-map.webp',
  'screenshot/2026-10-29-destop/客户端-录像回放.png': 'desktop-playback.webp',
  'public/architecture.png': 'architecture.webp'
}

const MAX_WIDTH = 1920

mkdirSync(outDir, { recursive: true })

const manifest = {}

for (const [src, out] of Object.entries(MAP)) {
  const srcPath = join(repoRoot, src)
  if (!existsSync(srcPath)) {
    console.warn(`[skip] 源文件不存在: ${src}`)
    continue
  }

  const meta = await sharp(srcPath).metadata()
  const info = await sharp(srcPath)
    .resize({ width: Math.min(meta.width ?? MAX_WIDTH, MAX_WIDTH), withoutEnlargement: true })
    .webp({ quality: 82 })
    .toFile(join(outDir, out))

  manifest[out] = { width: info.width, height: info.height, source: src }
  console.log(`[ok] ${src} -> ${out} (${info.width}x${info.height}, ${(info.size / 1024).toFixed(0)} KB)`)
}

writeFileSync(join(outDir, 'manifest.json'), JSON.stringify(manifest, null, 2))
console.log(`\n共生成 ${Object.keys(manifest).length} 张 WebP`)
