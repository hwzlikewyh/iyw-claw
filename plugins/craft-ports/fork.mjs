import { cp, mkdir, readFile, writeFile } from "node:fs/promises"
import { brotliCompressSync, constants, gunzipSync } from "node:zlib"
import { createHash } from "node:crypto"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { createRequire } from "node:module"
import { replaceBrandText } from "./wasm-brand.mjs"
import { adaptEditorBridge } from "./bridge-adapter.mjs"
import { adaptPanelLifecycle } from "./panel-adapter.mjs"
import { craftConfig } from "./config.mjs"
import { writeUsageFiles } from "./usage.mjs"

const VERSION = "0.1.0"
const digest = (value) => createHash("sha256").update(value).digest("hex")
const json = (value) => `${JSON.stringify(value, null, 2)}\n`

export async function prepareCraft(source, output) {
  const upstream = JSON.parse(
    await readFile(path.join(source, ".zcode-plugin/plugin.json"), "utf8")
  )
  const config = craftConfig(upstream)
  const dist = path.join(output, "dist")
  await mkdir(path.join(dist, "assets"), { recursive: true })
  await cp(path.join(source, "dist/licenses"), path.join(dist, "licenses"), {
    recursive: true,
    force: false,
    errorOnExist: true,
  })
  const assetFile = JSON.parse(
    await readFile(path.join(source, "dist/assets.json"), "utf8")
  )
  const assets = await convertAssets({
    source,
    dist,
    manifest: assetFile,
    config,
  })
  await writeFile(
    path.join(dist, "assets.json"),
    json({
      ...assetFile,
      name: config.name,
      version: VERSION,
      title: config.title,
      assets: assets.map((asset) => asset.entry),
    })
  )
  const originalServer = await readFile(
    path.join(source, "dist/server.mjs"),
    "utf8"
  )
  await writeFile(
    path.join(dist, "server.mjs"),
    adaptServer(originalServer, config)
  )
  await writeFile(
    path.join(dist, "panel.html"),
    adaptPanel(
      await readFile(path.join(source, "dist/panel.html"), "utf8"),
      config
    )
  )
  await writeRuntimeFiles({
    source,
    output,
    dist,
    originalServer,
    assets,
    upstream,
    config,
  })
}

async function writeRuntimeFiles({
  source,
  output,
  dist,
  originalServer,
  assets,
  upstream,
  config,
}) {
  await bundleStorage(dist)
  for (const file of ["tools.json", "BUILD.json"])
    await cp(path.join(source, "dist", file), path.join(dist, file), {
      force: false,
      errorOnExist: true,
    })
  await cp(
    fileURLToPath(new URL("./conversations.mjs", import.meta.url)),
    path.join(dist, "iyw-conversations.mjs")
  )
  await cp(
    fileURLToPath(new URL("./storage.mjs", import.meta.url)),
    path.join(dist, "iyw-storage.mjs")
  )
  await writeAuthorFiles(
    output,
    {
      upstream,
      serverSha256: digest(originalServer),
      assets: assets.map(({ originalSha256, entry, branding }) => ({
        originalSha256,
        sha256: entry.sha256,
        branding,
      })),
    },
    config
  )
}

async function convertAssets({ source, dist, manifest, config }) {
  const result = []
  for (const entry of manifest.assets) {
    const original = gunzipSync(
      await readFile(path.join(source, "dist/assets", `${entry.id}.gz`)),
      { maxOutputLength: entry.bytes }
    )
    if (original.length !== entry.bytes || digest(original) !== entry.sha256)
      throw Error("Upstream editor asset integrity mismatch")
    const modified =
      config.photo && entry.mime === "application/wasm"
        ? replaceBrandText(original)
        : { bytes: original, changes: [] }
    const id = digest(modified.bytes)
    const compressed = brotliCompressSync(modified.bytes, {
      params: config.photo
        ? { [constants.BROTLI_PARAM_QUALITY]: 8 }
        : {
            [constants.BROTLI_PARAM_QUALITY]: 9,
            [constants.BROTLI_PARAM_LGWIN]: 27,
            [constants.BROTLI_PARAM_LARGE_WINDOW]: true,
          },
    })
    await writeFile(path.join(dist, "assets", `${id}.br`), compressed)
    result.push({
      originalSha256: entry.sha256,
      entry: { ...entry, id, sha256: id },
      branding: modified.changes,
    })
  }
  return result
}

function replaceOne(source, before, after) {
  if (source.split(before).length !== 2)
    throw Error("Upstream craft adapter changed")
  return source.replace(before, after)
}

function adaptServer(source, config) {
  source = adaptEditorBridge(source, replaceOne)
  source = replaceOne(
    source,
    "var workspace = process.env.ZCODE_WORKSPACE_ROOT || process.cwd();",
    'var workspace = process.env.IYW_WORKSPACE_DIR; if (!workspace) throw Error("Original assistant workspace is required");'
  )
  source = replaceOne(
    source,
    'import { gunzipSync } from "node:zlib";',
    'import { brotliDecompressSync as gunzipSync } from "node:zlib";'
  )
  source = replaceOne(source, "`${id}.gz`", "`${id}.br`")
  if (config.photo)
    source = source.replaceAll('=== "photocraft"', `=== "${config.name}"`)
  if (!config.photo)
    source = replaceOne(
      source,
      "maxOutputLength: Math.max(1, asset.bytes)",
      "maxOutputLength: Math.max(1, asset.bytes), params: { 1: true }"
    )
  source = replaceOne(
    source,
    'ui: { prefersBorder: false, backgroundRetention: true },\n  "zcode/csp": { wasmUnsafeEval: true }',
    'ui: { prefersBorder: false, backgroundRetention: true, csp: { wasmUnsafeEval: true, resourceDomains: ["blob:", "data:"] } }'
  )
  source = replaceOne(
    source,
    "var metadata = {",
    "var conversationScope = installConversationScope(server);\ninstallEditorStorage(server, conversationScope, external_exports);\nvar metadata = {"
  )
  source = replaceOne(
    source,
    "  const bridge = new EditorBridge();",
    '  const bridge = conversationScope.proxy("bridge", () => new EditorBridge());'
  )
  source = replaceOne(
    source,
    "  const outputs = /* @__PURE__ */ new Map();",
    '  const outputs = conversationScope.proxy("outputs", () => new Map());'
  )
  source = replaceOne(
    source,
    'server.registerResource("panel", panelUri, {\n  mimeType: "text/html;profile=mcp-app",',
    `server.registerResource("panel", panelUri, {\n  title: "${config.title}",\n  mimeType: "text/html;profile=mcp-app",`
  )
  source = replaceOne(
    source,
    'server.registerResource("assets", `ui://${name}/assets.json`, {',
    'server.registerResource("managed-storage", `ui://${name}/storage-adapter.mjs`, { mimeType: "text/javascript" }, async () => ({ contents: [{ uri: `ui://${name}/storage-adapter.mjs`, mimeType: "text/javascript", text: await readFile3(join(directory, "storage-adapter.mjs"), "utf8") }] }));\nserver.registerResource("assets", `ui://${name}/assets.json`, {'
  )
  return (
    'import { installEditorStorage } from "./iyw-storage.mjs";\nimport { installConversationScope } from "./iyw-conversations.mjs";\n' +
    source
  )
}

function adaptPanel(source, config) {
  source = source.replaceAll(
    `name:"${config.source}",version:"0.2.4"`,
    `name:"${config.name}",version:"${VERSION}"`
  )
  if (config.photo)
    source = source.replaceAll('==="photocraft"', `==="${config.name}"`)
  source = source
    .replaceAll("PhotoCraft", "IYW Editor")
    .replaceAll("ZCode", "原助理")
    .replaceAll("50*1024*1024", "32*1024*1024")
    .replaceAll("50 MiB", "32 MiB")
  source = source.replaceAll("50 \\u004d", "32 \\u004d")
  source = replaceOne(
    source,
    "bC();let e=await SC(",
    'bC();const iywStorageSource=(await tt.readServerResource({uri:`${bk}/storage-adapter.mjs`})).contents[0].text;const iywStorageUrl=URL.createObjectURL(new Blob([iywStorageSource],{type:"text/javascript"}));try{iywStorage=await (await import(iywStorageUrl)).installManagedStorage((request)=>tt.callServerTool(request,{timeout:request.name==="_iyw_user_menu"?32e4:3e4}))}finally{URL.revokeObjectURL(iywStorageUrl)}let e=await SC('
  )
  if (!config.photo)
    source = replaceOne(
      source,
      'timeout:request.name==="_iyw_user_menu"?32e4:3e4}))',
      'timeout:request.name==="_iyw_user_menu"?32e4:3e4}),{presets:false,restoreInterface:false,compact:false,preferenceKey:"pdfcraft.preferences"})'
    )
  return adaptPanelLifecycle(source, replaceOne)
}

async function bundleStorage(dist) {
  const require = createRequire(new URL("../../package.json", import.meta.url))
  const esbuild = createRequire(
    createRequire(require.resolve("vitest/package.json")).resolve("vite")
  )("esbuild")
  await esbuild.build({
    entryPoints: [
      fileURLToPath(new URL("./storage-client/install.mjs", import.meta.url)),
    ],
    outfile: path.join(dist, "storage-adapter.mjs"),
    format: "esm",
    bundle: true,
    minify: true,
  })
}

async function writeAuthorFiles(output, provenance, config) {
  await mkdir(path.join(output, ".codex-plugin"))
  await writeFile(
    path.join(output, ".codex-plugin/plugin.json"),
    json({
      name: config.name,
      version: VERSION,
      description: `${config.title} 原助理适配版，支持交互编辑与导出`,
      license: "MIT OR Apache-2.0",
      skills: "skills",
      mcpServers: ".mcp.json",
    })
  )
  await writeFile(
    path.join(output, ".mcp.json"),
    json({
      mcpServers: {
        editor: {
          command: "node",
          args: ["dist/server.mjs"],
          cwd: ".",
          env: {},
          startup_timeout_sec: 180,
          tool_timeout_sec: 600,
        },
      },
    })
  )
  await writeUsageFiles(output, provenance, config)
}
