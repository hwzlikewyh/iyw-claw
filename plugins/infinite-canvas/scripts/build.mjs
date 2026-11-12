import { mkdir, readFile, readdir, writeFile } from "node:fs/promises"
import { rm } from "node:fs/promises"
import { build as buildRuntime } from "esbuild"
import { build as buildWorkbench } from "vite"
import { dirname, extname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("..", import.meta.url))
const entry = resolve(root, "runtime/src/main.ts")
const output = resolve(root, "runtime/dist/infinite-canvas-mcp.mjs")
const workbenchDirectory = resolve(root, "widget/.workbench-dist")
const rebuildWorkbench = process.env.INFINITE_CANVAS_SKIP_WORKBENCH_BUILD !== "1"
await mkdir(dirname(output), { recursive: true })
await buildRuntime({ entryPoints: [entry], outfile: output, bundle: true, platform: "node", target: "node20", format: "esm", packages: "bundle", sourcemap: false, banner: { js: "// Generated deterministic runtime bundle.\n" } })
if (rebuildWorkbench) await buildWorkbench({ configFile: resolve(root, "widget/vite.config.ts") })
const widget = await inlineWorkbench(workbenchDirectory)
const widgetPath = resolve(root, "widget/dist/infinite-canvas-widget.html")
await mkdir(dirname(widgetPath), { recursive: true })
await writeFile(widgetPath, widget, "utf8")
if (rebuildWorkbench) await rm(workbenchDirectory, { recursive: true, force: true })
if (rebuildWorkbench) await rm(workbenchDirectory, { recursive: true, force: true })
const bytes = (await readFile(output)).byteLength
console.log(JSON.stringify({ runtime: output, runtimeBytes: bytes, widget: widgetPath }, null, 2))

async function inlineWorkbench(directory) {
  const html = await readFile(join(directory, "workbench.html"), "utf8")
  const names = await readdir(join(directory, "assets"))
  const javascript = names.find((name) => extname(name) === ".js")
  const stylesheet = names.find((name) => extname(name) === ".css")
  if (!javascript || !stylesheet) throw new Error("workbench bundle is incomplete")
  const [script, styles] = await Promise.all([
    readFile(join(directory, "assets", javascript), "utf8"),
    readFile(join(directory, "assets", stylesheet), "utf8"),
  ])
  const embeddedScript = await inlineModelIcons(script)
  const scriptTag = html.match(/<script[^>]+src="([^"]+)"[^>]*><\/script>/)
  const styleTag = html.match(/<link[^>]+stylesheet[^>]+href="([^"]+)"[^>]*>/)
  if (!scriptTag || !styleTag) throw new Error("workbench html does not reference its bundles")
  const result = html
    .replace(scriptTag[0], () => `<script type="module">${embeddedScript.replaceAll("</script>", "<\\/script>")}</script>`)
    .replace(styleTag[0], () => `<style>${styles}</style>`)
    .replace(/<link[^>]+modulepreload[^>]*>/g, "")
  return result
}

async function inlineModelIcons(source) {
  const directory = resolve(root, "vendor/infinite-canvas-web/public/icons")
  let result = source
  for (const name of ["claude", "deepseek", "gemini", "glm", "grok", "openai"]) {
    const bytes = await readFile(join(directory, `${name}.svg`))
    const dataUrl = `data:image/svg+xml;base64,${bytes.toString("base64")}`
    result = result.replaceAll(JSON.stringify(`/icons/${name}.svg`), JSON.stringify(dataUrl))
  }
  if (/"\/icons\/(claude|deepseek|gemini|glm|grok|openai)\.svg"/.test(result)) throw new Error("model provider icons were not embedded")
  return result
}
