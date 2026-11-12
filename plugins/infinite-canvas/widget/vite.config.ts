import { resolve } from "node:path"
import { fileURLToPath } from "node:url"
import react from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/postcss"
import { defineConfig } from "vite"

const widgetRoot = fileURLToPath(new URL(".", import.meta.url))
const vendorRoot = resolve(widgetRoot, "../vendor/infinite-canvas-web")

export default defineConfig({
  root: widgetRoot,
  base: "./",
  publicDir: false,
  plugins: [react()],
  resolve: { alias: { "@": resolve(vendorRoot, "src") } },
  css: { postcss: { plugins: [tailwindcss()] } },
  define: {
    __APP_VERSION__: JSON.stringify("0.20.0"),
    __APP_RELEASES__: "[]",
  },
  build: {
    outDir: resolve(widgetRoot, ".workbench-dist"),
    emptyOutDir: true,
    sourcemap: false,
    cssCodeSplit: false,
    assetsInlineLimit: 100_000_000,
    rollupOptions: { input: resolve(widgetRoot, "workbench.html"), output: { inlineDynamicImports: true } },
  },
})
