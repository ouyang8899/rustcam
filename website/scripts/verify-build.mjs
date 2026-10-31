import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { join, resolve } from 'node:path'

const root = resolve(process.cwd(), '.output', 'public')
const failures = []

function fail(message) {
  failures.push(message)
}

if (!existsSync(root)) {
  console.error(`构建产物不存在：${root}\n请先执行 npm run build`)
  process.exit(1)
}

const ROUTES = [
  { path: '/', file: 'index.html' },
  { path: '/features/', file: 'features/index.html' },
  { path: '/scenarios/', file: 'scenarios/index.html' },
  { path: '/docs/', file: 'docs/index.html' },
  { path: '/about/', file: 'about/index.html' },
  { path: '/en/', file: 'en/index.html' },
  { path: '/en/features/', file: 'en/features/index.html' },
  { path: '/en/scenarios/', file: 'en/scenarios/index.html' },
  { path: '/en/docs/', file: 'en/docs/index.html' },
  { path: '/en/about/', file: 'en/about/index.html' }
]

for (const route of ROUTES) {
  const file = join(root, route.file)
  if (!existsSync(file)) {
    fail(`缺少路由产物: ${route.path} → ${route.file}`)
    continue
  }
  if (statSync(file).size < 1024) {
    fail(`路由产物过小（疑似空壳）: ${route.path}`)
  }
}

for (const asset of ['sitemap.xml', 'robots.txt', 'favicon.svg']) {
  if (!existsSync(join(root, asset))) {
    fail(`缺少静态资源：${asset}`)
  }
}

const imgDir = join(root, 'img')
const images = existsSync(imgDir)
  ? readdirSync(imgDir).filter((name) => name.endsWith('.webp'))
  : []

if (images.length < 10) {
  fail(`图片数量不足: 期望至少 10 张 WebP，实际 ${images.length} 张`)
}

for (const route of ROUTES) {
  const file = join(root, route.file)
  if (!existsSync(file)) continue
  const html = readFileSync(file, 'utf8')
  const refs = html.match(/\/img\/[A-Za-z0-9._-]+\.webp/g) ?? []
  for (const ref of new Set(refs)) {
    if (!existsSync(join(root, ref.replace(/^\//, '')))) {
      fail(`${route.path} 引用了不存在的图片：${ref}`)
    }
  }
}

const zhHome = readFileSync(join(root, 'index.html'), 'utf8')
const enHome = readFileSync(join(root, 'en', 'index.html'), 'utf8')

if (!/href="\/en(\/|")/.test(zhHome)) {
  fail('中文首页缺少指向英文站的链接')
}
if (!enHome.includes('href="/"')) {
  fail('英文首页缺少指回中文站的链接')
}

if (failures.length) {
  console.error(`\n校验失败：${failures.length} 项`)
  for (const item of failures) console.error(`  ✗ ${item}`)
  process.exit(1)
}

console.log(`校验通过：${ROUTES.length} 条路由、${images.length} 张图片、sitemap/robots 齐全`)
