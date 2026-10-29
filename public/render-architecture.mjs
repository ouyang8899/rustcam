// 将 public/architecture.mmd 渲染为 public/architecture.png
//
// 用法:
//   node public/render-architecture.mjs
//
// 依赖: playwright (仓库根目录 node_modules) + Mermaid UMD (CDN, 自动缓存到系统临时目录)

import { chromium } from 'playwright'
import { readFile, mkdir } from 'node:fs/promises'
import { existsSync, createWriteStream } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { tmpdir } from 'node:os'
import path from 'node:path'
import https from 'node:https'

const HERE = path.dirname(fileURLToPath(import.meta.url))
const MMD_PATH = path.join(HERE, 'architecture.mmd')
const PNG_PATH = path.join(HERE, 'architecture.png')
const CACHE_PATH = path.join(tmpdir(), 'mermaid-11.6.0.min.js')
const MERMAID_URL = 'https://cdn.jsdelivr.net/npm/mermaid@11.6.0/dist/mermaid.min.js'
const SCALE = 2

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest)
    https
      .get(url, (res) => {
        if (res.statusCode !== 200) {
          file.close()
          reject(new Error(`下载失败 ${url} -> HTTP ${res.statusCode}`))
          return
        }
        res.pipe(file)
        file.on('finish', () => file.close(() => resolve(dest)))
      })
      .on('error', (err) => {
        file.close()
        reject(err)
      })
  })
}

async function loadMermaid() {
  if (existsSync(CACHE_PATH)) return readFile(CACHE_PATH, 'utf8')
  await mkdir(tmpdir(), { recursive: true })
  await download(MERMAID_URL, CACHE_PATH)
  return readFile(CACHE_PATH, 'utf8')
}

const source = await readFile(MMD_PATH, 'utf8')
const mermaidJs = await loadMermaid()

const browser = await chromium.launch()
const page = await browser.newPage({ viewport: { width: 1600, height: 1200 }, deviceScaleFactor: SCALE })

await page.setContent(
  `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8">
   <style>
     html,body { margin:0; padding:0; background:#ffffff; }
     #host { display:inline-block; padding:24px; }
     #host svg { display:block; max-width:none !important; }
   </style></head><body><div id="host"></div></body></html>`,
  { waitUntil: 'domcontentloaded' },
)

await page.addScriptTag({ content: mermaidJs })

// 提取 .mmd 里的 %%{init: {...}}%% 配置并通过 initialize 注入
// (Mermaid v11 在 render() 内不会应用指令配置, 必须显式传入)
const directive = source.match(/%%\{init:\s*([\s\S]*?)\s*\}%%/)
const initConfig = directive ? JSON.parse(directive[1]) : {}

const error = await page.evaluate(async ({ src, cfg }) => {
  const mermaid = window.mermaid && window.mermaid.default ? window.mermaid.default : window.mermaid
  if (!mermaid || typeof mermaid.render !== 'function') return 'window.mermaid 未正确加载'
  mermaid.initialize({ startOnLoad: false, securityLevel: 'strict', ...cfg })
  try {
    const { svg } = await mermaid.render('rustcam-arch', src)
    document.getElementById('host').innerHTML = svg
    return null
  } catch (e) {
    return e && e.message ? e.message : String(e)
  }
}, { src: source, cfg: initConfig })

if (error) {
  await browser.close()
  console.error('[architecture] Mermaid 渲染失败:\n' + error)
  process.exit(1)
}

await page.waitForSelector('#host svg', { timeout: 15000 })

const box = await page.locator('#host svg').boundingBox()
if (!box || box.width < 10 || box.height < 10) {
  await browser.close()
  console.error('[architecture] 渲染结果尺寸异常')
  process.exit(1)
}

await page.locator('#host svg').screenshot({ path: PNG_PATH })
await browser.close()

console.log(`[architecture] ${path.relative(process.cwd(), PNG_PATH)}  ${Math.round(box.width)}x${Math.round(box.height)} CSS px @${SCALE}x`)
