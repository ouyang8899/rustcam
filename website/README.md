# RustCam 官网

RustCam-Media 的官方推广站点，基于 Nuxt 4 静态生成，中英双语。

## 仓库地址

| 平台 | 地址 |
|---|---|
| Gitee | https://gitee.com/ouliuying/rustcam |
| GitHub | https://github.com/ouyang8899/rustcam |

- 官网：https://www.rustcam.com
- 联系邮箱：ouyang.8899@hotmail.com

## 技术栈

| 关注点 | 选型 |
|---|---|
| 框架 | Nuxt 4（SSR + `nuxi generate` 静态生成） |
| 语言 | JavaScript（无 TypeScript） |
| 国际化 | `@nuxtjs/i18n` v10（中文 `/`，英文 `/en/`） |
| 样式 | Tailwind CSS v4（`@tailwindcss/vite`） |
| SEO | `@nuxtjs/sitemap` + `@nuxtjs/robots` |
| 图片 | `sharp` 将仓库截图批量转 WebP |

## 目录结构

```
website/
├── app/
│   ├── components/home/    # 首页区块
│   ├── components/site/    # 头部 / 页脚 / 语言切换
│   ├── components/ui/      # 展示组件
│   ├── layouts/default.vue # 全站布局
│   ├── pages/              # 5 个页面
│   ├── utils/locale.js     # pickLocale 内容选择器
│   └── assets/css/main.css # 设计 token
├── content/                # 长文案数据模块（zh / en 并列）
├── i18n/locales/           # UI 词条（zh.js / en.js）
├── public/img/             # 生成的 WebP 图片
└── scripts/                # 图片管线与产物校验
```

内容模块位于 `website/content/`，在组件中用根目录别名导入：`import { links } from '~~/content/shared'`。

## 命令

```bash
npm install        # 安装依赖
npm run dev        # 开发服务器 http://localhost:3000
npm run build      # 静态生成 → .output/public
npm run preview    # 本地预览静态产物
npm run images     # 从 ../screenshot 重新生成 WebP
npm run verify     # 校验构建产物完整性
```

## 环境变量

| 变量 | 默认值 | 说明 |
|---|---|---|
| `NUXT_APP_BASE_URL` | `/` | 子路径部署时的站点根路径 |
| `NUXT_APP_SITE_URL` | `https://www.rustcam.com` | 正式域名，用于 sitemap 与 OG 绝对地址 |

## 部署

Cloudflare Pages / Vercel / Netlify 通用：

- 构建命令：`cd website && npm ci && npm run build`
- 输出目录：`website/.output/public`
- 环境变量：`NUXT_APP_SITE_URL=https://<你的域名>`

产物为纯静态文件，可直接托管到任意静态服务器或对象存储。

## 修改文案

- 页面长文案：`content/<page>.js`，中英并列（`zh` / `en` 两个顶层键）。
- 导航、页脚、按钮等 UI 词条：`i18n/locales/zh.js` 与 `i18n/locales/en.js`。
- 站点外链（仓库、邮箱）统一在 `content/shared.js` 的 `links` 中定义。

## 更新截图

仓库截图位于 `../screenshot`，新增截图后在 `scripts/prepare-images.mjs` 的 `MAP` 中登记中文源文件名与 ASCII 输出名，再执行：

```bash
npm run images
```

输出 WebP 写入 `public/img/`，尺寸记录在 `public/img/manifest.json`。
