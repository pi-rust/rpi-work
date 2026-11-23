import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'
import { defineConfig } from 'vite'

// No implicit connection to a developer's installed node. Tests and local
// development must select their own explicit target; hosted assets need none.
const nodeTarget = process.env.RPI_NODE_URL?.trim()
const proxy = nodeTarget ? { '/api': { target: nodeTarget, changeOrigin: true, ws: true } } : undefined

export default defineConfig(({ mode }) => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      cn: path.resolve(__dirname, './src/lib/utils.ts'),
      // Product module graph never imports a test fixture. Fixture output is
      // separate and still requires ?fixture=1 to activate the sample.
      '@workbench/initial-snapshot': path.resolve(__dirname, mode === 'fixture' ? './tests/fixtures/workspace.ts' : './src/lib/initial-snapshot.ts'),
    },
  },
  build: { outDir: mode === 'fixture' ? 'dist-fixture' : 'dist' },
  server: { proxy },
  preview: { proxy },
}))
