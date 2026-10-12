import path from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { compression } from 'vite-plugin-compression2'

const ROUTER = process.env.ROUTER ?? 'http://192.168.1.1:8088'

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    compression({ algorithms: ['gzip', 'brotliCompress'], exclude: [/\.(png|woff2)$/], threshold: 1024 }),
  ],
  resolve: { alias: { '@': path.resolve(__dirname, './src'), '@schema': path.resolve(__dirname, '../schema') } },
  build: { target: 'es2022', cssCodeSplit: false, chunkSizeWarningLimit: 400 },
  server: {
    // The router checks Origin == Host, so present the proxied request as same-origin.
    proxy: { '/api': { target: ROUTER, changeOrigin: true, headers: { origin: ROUTER } } },
  },
})
