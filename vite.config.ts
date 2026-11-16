import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [
    svelte(),
    {
      // Dev only: `?shot` pages load this, which holds the load event until the UI has settled,
      // so a headless browser's screenshot shows the finished page.
      name: 'slow-pixel',
      configureServer(server) {
        server.middlewares.use('/__slow', (_req, res) => setTimeout(() => res.end(), 2500))
      },
    },
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  build: { target: 'esnext', chunkSizeWarningLimit: 900 },
})
