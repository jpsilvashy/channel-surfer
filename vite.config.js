import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  root: 'frontend',
  server: {
    port: 3000
  },
  publicDir: '../public',
  build: {
    outDir: '../dist'
  }
})
