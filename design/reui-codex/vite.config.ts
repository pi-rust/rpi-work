import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import { defineConfig } from 'vite'

// Same-origin node proxy: the wasm client talks to `${origin}/api/v1/*`, so the
// browser never needs CORS or a second host allowlist entry.
const nodeTarget = process.env.RPI_NODE_URL ?? 'http://127.0.0.1:13031'
const proxy = { '/api': { target: nodeTarget, changeOrigin: true, ws: true } }

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      // ReUI registry sources import `cn` as a bare alias.
      cn: path.resolve(__dirname, './src/lib/utils.ts'),
    },
  },
  server: { proxy },
  preview: { proxy },
})
