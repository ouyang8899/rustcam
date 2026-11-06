import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { viteStaticCopy } from 'vite-plugin-static-copy'
import { resolve } from 'path'

export default defineConfig({
  root: __dirname,
  plugins: [
    vue(),
    // Cesium 的 Workers/Assets/Widgets 必须作为静态资源发布到 /cesium/
    viteStaticCopy({
      targets: [
        { src: 'node_modules/cesium/Build/Cesium/Workers', dest: 'cesium' },
        { src: 'node_modules/cesium/Build/Cesium/ThirdParty', dest: 'cesium' },
        { src: 'node_modules/cesium/Build/Cesium/Assets', dest: 'cesium' },
        { src: 'node_modules/cesium/Build/Cesium/Widgets', dest: 'cesium' },
        // MapLibre 的 Worker 也必须作为静态资源发布：它需要与 shared 模块同目录。
        { src: 'node_modules/maplibre-gl/dist/maplibre-gl-worker.mjs', dest: 'maplibre' },
        { src: 'node_modules/maplibre-gl/dist/maplibre-gl-shared.mjs', dest: 'maplibre' },
      ],
    }),
  ],
  css: {
    preprocessorOptions: {
      scss: {
        api: 'modern-compiler',
      },
    },
  },
  server: {
    port: 5173,
    watch: {
      // Windows 下编辑器/杀毒软件会短暂锁住文件，轮询可避免 EBUSY 导致 dev server 退出
      usePolling: true,
      interval: 300,
    },
    proxy: {
      '/api': {
        target: 'http://localhost:8081',
        changeOrigin: true,
      },
      '/ws': {
        target: 'ws://localhost:8081',
        ws: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    rollupOptions: {
      input: resolve(__dirname, 'index.html'), // 指向 public 目录下的 index.html
    },
  },
  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
    },
  },
})
