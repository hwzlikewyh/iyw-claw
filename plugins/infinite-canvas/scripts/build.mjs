import { mkdir, readFile, writeFile } from "node:fs/promises"
import { build } from "esbuild"
import { dirname, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("..", import.meta.url))
const entry = resolve(root, "runtime/src/main.ts")
const output = resolve(root, "runtime/dist/infinite-canvas-mcp.mjs")
await mkdir(dirname(output), { recursive: true })
await build({ entryPoints: [entry], outfile: output, bundle: true, platform: "node", target: "node20", format: "esm", packages: "bundle", sourcemap: false, banner: { js: "// Generated deterministic runtime bundle.\n" } })
const widgetBundle = await build({ entryPoints: [resolve(root, "widget/src/main.ts")], bundle: true, platform: "browser", target: "es2022", format: "iife", write: false, minify: true })
const bundleText = widgetBundle.outputFiles[0]?.text
if (!bundleText) throw new Error("widget bundle is empty")
const widget = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Infinite Canvas</title></head><body><main id="app"></main><script>${bundleText.replaceAll("</script>", "<\\/script>")}</script></body></html>\n`
const widgetPath = resolve(root, "widget/dist/infinite-canvas-widget.html")
await mkdir(dirname(widgetPath), { recursive: true })
await writeFile(widgetPath, widget, "utf8")
const bytes = (await readFile(output)).byteLength
console.log(JSON.stringify({ runtime: output, runtimeBytes: bytes, widget: widgetPath }, null, 2))
