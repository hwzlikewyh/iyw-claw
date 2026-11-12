import { cp, mkdir, readFile, writeFile } from "node:fs/promises"
import { brotliCompressSync, constants, gunzipSync } from "node:zlib"
import { createHash } from "node:crypto"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { createRequire } from "node:module"
import { replaceBrandText } from "./wasm-brand.mjs"
import { adaptEditorBridge } from "./bridge-adapter.mjs"
import { adaptPanelLifecycle } from "./panel-adapter.mjs"

const VERSION = "0.1.0",
  NAME = "iyw-image-editor",
  TITLE = "图像编辑"
const digest = (value) => createHash("sha256").update(value).digest("hex")
const json = (value) => `${JSON.stringify(value, null, 2)}\n`

export async function prepareCraft(source, output) {
  const upstream = JSON.parse(
    await readFile(path.join(source, ".zcode-plugin/plugin.json"), "utf8")
  )
  if (upstream.name !== "photocraft" || upstream.version !== "0.2.4")
    throw Error("Unexpected PhotoCraft source version")
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
  const assets = await convertAssets(source, dist, assetFile)
  await writeFile(
    path.join(dist, "assets.json"),
    json({
      ...assetFile,
      name: NAME,
      version: VERSION,
      title: TITLE,
      assets: assets.map((asset) => asset.entry),
    })
  )
  const originalServer = await readFile(
    path.join(source, "dist/server.mjs"),
    "utf8"
  )
  await writeFile(path.join(dist, "server.mjs"), adaptServer(originalServer))
  await writeFile(
    path.join(dist, "panel.html"),
    adaptPanel(await readFile(path.join(source, "dist/panel.html"), "utf8"))
  )
  await writeRuntimeFiles({
    source,
    output,
    dist,
    originalServer,
    assets,
    upstream,
  })
}

async function writeRuntimeFiles({
  source,
  output,
  dist,
  originalServer,
  assets,
  upstream,
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
  await writeAuthorFiles(output, {
    upstream,
    serverSha256: digest(originalServer),
    assets: assets.map(({ originalSha256, entry, branding }) => ({
      originalSha256,
      sha256: entry.sha256,
      branding,
    })),
  })
}

async function convertAssets(source, dist, manifest) {
  const result = []
  for (const entry of manifest.assets) {
    const original = gunzipSync(
      await readFile(path.join(source, "dist/assets", `${entry.id}.gz`)),
      { maxOutputLength: entry.bytes }
    )
    if (original.length !== entry.bytes || digest(original) !== entry.sha256)
      throw Error("Upstream editor asset integrity mismatch")
    const modified =
      entry.mime === "application/wasm"
        ? replaceBrandText(original)
        : { bytes: original, changes: [] }
    const id = digest(modified.bytes)
    const compressed = brotliCompressSync(modified.bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 8 },
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

function adaptServer(source) {
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
  source = source.replaceAll('=== "photocraft"', `=== "${NAME}"`)
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
    `server.registerResource("panel", panelUri, {\n  title: "${TITLE}",\n  mimeType: "text/html;profile=mcp-app",`
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

function adaptPanel(source) {
  source = source
    .replaceAll(
      'name:"photocraft",version:"0.2.4"',
      `name:"${NAME}",version:"${VERSION}"`
    )
    .replaceAll('==="photocraft"', `==="${NAME}"`)
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

async function writeAuthorFiles(output, provenance) {
  await mkdir(path.join(output, ".codex-plugin"))
  await writeFile(
    path.join(output, ".codex-plugin/plugin.json"),
    json({
      name: NAME,
      version: VERSION,
      description: "原助理图像编辑器，基于PhotoCraft修改，支持交互编辑与导出",
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
  await writeUsageFiles(output, provenance)
}

async function writeUsageFiles(output, provenance) {
  await mkdir(path.join(output, "skills/image-editor"), { recursive: true })
  await writeFile(
    path.join(output, "skills/image-editor/SKILL.md"),
    `---\nname: iyw-image-editor\ndescription: 在当前项目内打开、编辑和导出图像，通过原助理图像编辑插件与用户共用画布。\n---\n\n# 图像编辑\n\n通过search_iyw_capabilities发现插件能力，再用invoke_iyw_capability按capability_id调用。open_editor仅打开页面，不代表已经编辑。可选path必须是当前项目相对路径。页面就绪后才调用编辑工具；先读取文档状态，逐项核对结果，超时后查询实际状态，不自动重试写入。\n\n用户手动导出使用原助理保存确认，保存到当前设备。模型工具写项目文件前需要当前项目写入授权，已有路径保留原覆盖行为并须用户明确要求。不要把打开、发出下载或得到文件名当作落盘成功。\n\n此插件基于PhotoCraft 0.2.4修改，非ArtCraft官方产品；原许可及第三方声明在source/dist/licenses目录，修改记录见source/port-provenance.json。\n`
  )
  await writeFile(
    path.join(output, "port-provenance.json"),
    json({
      name: NAME,
      version: VERSION,
      source: "photocraft@0.2.4",
      ...provenance,
      changes: [
        "iyw-workspace-and-name",
        "static-data-brand-text",
        "conversation-editor-bridge",
        "brotli-asset-integrity",
        "host-confirmed-export",
        "managed-private-editor-storage",
        "restore-interface-after-upstream-bootstrap",
      ],
    })
  )
  await writeFile(
    path.join(output, "README.md"),
    `# 原助理图像编辑\n\n基于PhotoCraft修改的独立插件，非ArtCraft官方构建。MIT/Apache-2.0及第三方许可证完整保留在dist/licenses。受限ArtCraft文字标识从运行资产移除，PhotoCraft应用图标按其独立MIT/Apache-2.0许可证保留。WASM仅修改静态数据段的等长品牌文字并验证模块；Brotli压缩不删除任何许可，解压后仍按实际摘要校验。\n\n用户从云端自主选择安装和授权，不预置。当前为待验收制品，未发布；手动导出需原助理宿主保存能力。旧客户端须在发布端设置最低版本门禁。\n`
  )
}
