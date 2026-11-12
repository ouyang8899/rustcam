import { existsSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const here = dirname(fileURLToPath(import.meta.url))
const siteRoot = resolve(here, '..')
const repoRoot = resolve(siteRoot, '..')
const outDir = join(siteRoot, 'public', 'img')

/** 源文件（相对仓库根） → [输出文件名, 最大宽度] */
const MAP = {
  // Web 控制台
  'screenshot/2026-10-29/系统概览.png': ['console-overview.webp', 1920],
  'screenshot/2026-10-29/设备管理-设备列表.png': ['console-devices.webp', 1920],
  'screenshot/2026-10-29/设备管理-通道列表.png': ['console-channels.webp', 1920],
  'screenshot/2026-10-29/设备列表-电子地图-视频查看.png': ['console-map-video.webp', 1920],
  'screenshot/2026-10-29/监控墙-监控墙.png': ['console-tvwall.webp', 1920],
  'screenshot/2026-10-29/录像管理-录像回放.png': ['console-playback.webp', 1920],
  'screenshot/2026-10-29/报警记录.png': ['console-alarms.webp', 1920],
  'screenshot/2026-10-29/媒体服务器.png': ['console-media-servers.webp', 1920],
  'screenshot/2026-10-29/管理-用户管理.png': ['console-users.webp', 1920],

  // 移动眼
  'screenshot/2026-11-10/移动眼-实时飞控.png': ['mobile-eye-live.webp', 1600],
  'screenshot/2026-11-10/移动眼-无人机列表.png': ['mobile-eye-drones.webp', 1600],
  'screenshot/2026-11-10/移动眼-航线规划.png': ['mobile-eye-route.webp', 1600],
  'screenshot/2026-11-10/移动眼-电子围栏.png': ['mobile-eye-fence.webp', 1600],
  'screenshot/2026-11-10/移动眼-轨迹回放.png': ['mobile-eye-track.webp', 1600],
  'screenshot/2026-11-10/移动眼-控制权管理.png': ['mobile-eye-control.webp', 1600],

  // Qt 桌面客户端
  'screenshot/2026-10-29-destop/客户端-实时预览.png': ['desktop-live.webp', 1920],
  'screenshot/2026-10-29-destop/客户端-电子地图.png': ['desktop-map.webp', 1920],
  'screenshot/2026-10-29-destop/客户端-录像回放.png': ['desktop-playback.webp', 1920],

  'public/architecture.png': ['architecture.webp', 1920]
}

mkdirSync(outDir, { recursive: true })

const manifest = {}

for (const [src, [out, maxWidth]] of Object.entries(MAP)) {
  const srcPath = join(repoRoot, src)
  if (!existsSync(srcPath)) {
    console.warn(`[skip] 源文件不存在: ${src}`)
    continue
  }

  const meta = await sharp(srcPath).metadata()
  const info = await sharp(srcPath)
    .resize({ width: Math.min(meta.width ?? maxWidth, maxWidth), withoutEnlargement: true })
    .webp({ quality: 82 })
    .toFile(join(outDir, out))

  manifest[out] = { width: info.width, height: info.height, source: src }
  console.log(`[ok] ${src} -> ${out} (${info.width}x${info.height}, ${(info.size / 1024).toFixed(0)} KB)`)
}

writeFileSync(join(outDir, 'manifest.json'), JSON.stringify(manifest, null, 2))
console.log(`\n共生成 ${Object.keys(manifest).length} 张 WebP`)
