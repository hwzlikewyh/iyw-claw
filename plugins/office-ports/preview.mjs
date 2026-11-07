import { createServer } from "node:http"
import { createRequire } from "node:module"
import { randomUUID } from "node:crypto"
import { mkdir, mkdtemp, readFile, realpath, rm } from "node:fs/promises"
import path from "node:path"
import { fileURLToPath } from "node:url"
import { PluginRpc } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/rpc.mjs"

const MAX_BODY_BYTES = 64 * 1024 * 1024
const PREVIEW_CONVERSATION_ID = 1
const here = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(here, "../..")
const require = createRequire(path.join(root, "package.json"))
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce(
      (values, value, index, all) =>
        index % 2 ? values : [...values, [value, all[index + 1]]],
      []
    )
)
if (!args["--package"] || !args["--workspace"] || !args["--data"])
  throw Error(
    "Use --package <package-root> --workspace <workspace> --data <preview-data> [--port <port>]"
  )
const port = Number(args["--port"] ?? "18971")
if (!Number.isInteger(port) || port < 1024 || port > 65535)
  throw Error("Invalid preview port")
const origin = `http://127.0.0.1:${port}`
const packageRoot = await realpath(args["--package"])
const workspace = await realpath(args["--workspace"])
const data = path.resolve(args["--data"])
await mkdir(data, { recursive: true })
const profile = await mkdtemp(path.join(data, ".office-preview-"))
const storage = args["--storage"] ? path.resolve(args["--storage"]) : profile
await mkdir(storage, { recursive: true })
const manifest = JSON.parse(
  await readFile(path.join(packageRoot, ".iyw-plugin.json"), "utf8")
)
const native = JSON.parse(
  await readFile(path.join(packageRoot, "native-plugin.json"), "utf8")
)
const runtime = manifest.components.runtimes[0]
const client = new PluginRpc({
  entrypoint: path.join(packageRoot, runtime.entrypoint),
  cwd: packageRoot,
  env: {
    IYW_WORKSPACE_DIR: workspace,
    IYW_PLUGIN_DATA_DIR: storage,
    HOME: profile,
    USERPROFILE: profile,
    APPDATA: profile,
    LOCALAPPDATA: profile,
    XDG_CONFIG_HOME: profile,
    XDG_CACHE_HOME: profile,
  },
  timeout: 30_000,
})
await client.initialize()
const initialResult = args["--open"]
  ? await client.call("tools/call", {
      name: "open_document",
      arguments: { path: args["--open"] },
      _meta: { "iyw/conversationId": String(PREVIEW_CONVERSATION_ID) },
    })
  : null
if (initialResult?.isError) {
  await client.close()
  if (
    path.dirname(profile) === data &&
    path.basename(profile).startsWith(".office-preview-")
  )
    await rm(profile, { recursive: true, force: true })
  const message =
    initialResult.content?.find((value) => value.type === "text")?.text ??
    "Unknown document error"
  throw Error(
    `The preview document could not be opened: ${message.slice(0, 300)}`
  )
}
const resource = await client.call("resources/read", {
  uri: manifest.components.apps[0].resourceUri,
})
const page = resource.contents[0]
const launch = {
  instanceId: randomUUID(),
  conversationId: PREVIEW_CONVERSATION_ID,
  leaseToken: randomUUID(),
  nonce: randomUUID(),
  launchPayload: initialResult
    ? { arguments: { path: args["--open"] }, result: initialResult }
    : {},
}
const tab = {
  id: launch.instanceId,
  conversationId: launch.conversationId,
  label:
    native.connectors[manifest.components.connectors[0].key].resources[0]
      ?.title ?? manifest.name,
  pluginSlug: manifest.name,
  appKey: manifest.components.apps[0].key,
  response: {
    html: page.text,
    launch,
    hostVersion: "author-preview",
    resourceMeta: page._meta.ui,
  },
}
const esbuild = createRequire(
  createRequire(require.resolve("vitest/package.json")).resolve("vite")
)("esbuild")
const bundle = await esbuild.build({
  entryPoints: [path.join(here, "preview-entry.tsx")],
  write: false,
  bundle: true,
  format: "esm",
  jsx: "automatic",
  absWorkingDir: root,
  alias: { "@": path.join(root, "src") },
  define: { "process.env.NODE_ENV": '"production"' },
})
const cssSource =
  (await readFile(path.join(root, "src/app/globals.css"), "utf8")).replace(
    '@import "tailwindcss";',
    '@import "tailwindcss" source(none);'
  ) +
  `\n@source "${root}/src/components/message";\n@source "${root}/src/components/layout/plugin-panel-frame.tsx";\n@source "${root}/src/components/ui";`
const stylesheet = await require("postcss")([
  require("@tailwindcss/postcss")({ base: root }),
]).process(cssSource, { from: path.join(root, "src/app/globals.css") })
const html =
  '<!doctype html><html lang="zh-CN"><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="/preview.css"><style>html,body,#root{height:100%;margin:0}#root{display:flex;flex-direction:column}header{padding:12px 20px;background:#f4f4f4}h1{font-size:20px;margin:0}header p{margin:6px 0}main{min-height:0;flex:1;display:flex}</style><body class="font-sans"><div id="root"></div><script type="module" src="/preview.js"></script></body></html>'

const server = createServer(async (request, response) => {
  try {
    if (request.method === "GET" && request.url === "/")
      return send(response, "text/html", html)
    if (request.method === "GET" && request.url === "/preview.js")
      return send(response, "text/javascript", bundle.outputFiles[0].contents)
    if (request.method === "GET" && request.url === "/preview.css")
      return send(
        response,
        "text/css",
        stylesheet.css.replace(
          /url\([^)]*\/files\/([^/)]+\.woff2)\)/g,
          "url(/files/$1)"
        )
      )
    if (
      request.method === "GET" &&
      /^\/files\/[a-z0-9-]+\.woff2$/.test(request.url)
    ) {
      for (const family of [
        "inter",
        "geist",
        "jetbrains-mono",
        "fira-code",
        "geist-mono",
      ]) {
        const bytes = await readFile(
          path.join(
            root,
            "node_modules/@fontsource-variable",
            family,
            request.url.slice(1)
          )
        ).catch((cause) => {
          if (cause.code === "ENOENT") return null
          throw cause
        })
        if (bytes) return send(response, "font/woff2", bytes)
      }
    }
    if (request.method === "GET" && request.url === "/bootstrap")
      return send(response, "application/json", JSON.stringify(tab))
    if (request.method !== "POST" || request.headers.origin !== origin) {
      response.writeHead(403).end()
      return
    }
    const payload = JSON.parse(await readBody(request))
    if (request.url === "/api/plugin_app_teardown")
      return send(response, "application/json", "null")
    if (request.url !== "/api/plugin_app_message") {
      response.writeHead(404).end()
      return
    }
    const result = await route(payload.request)
    send(response, "application/json", JSON.stringify(result))
  } catch (cause) {
    response
      .writeHead(400, { "Content-Type": "application/json" })
      .end(JSON.stringify({ message: cause.message }))
  }
})
server.listen(port, "127.0.0.1", () =>
  console.log(`Office author preview: ${origin}`)
)

async function route(request) {
  if (
    request.instanceId !== launch.instanceId ||
    request.leaseToken !== launch.leaseToken ||
    request.nonce !== launch.nonce
  )
    throw Error("Preview session changed")
  let result = {}
  if (request.method === "resources/read") {
    const uri = request.params?.uri
    const prefix = page.uri.slice(0, page.uri.indexOf("/", "ui://".length) + 1)
    if (
      typeof uri !== "string" ||
      !uri.startsWith(prefix) ||
      /[%?#\\]|(?:^|\/)\.{1,2}(?:\/|$)/.test(uri)
    )
      throw Error("Resource outside preview plugin")
    result = await client.call(request.method, request.params)
  } else if (request.method === "resources/list")
    result = await client.call(request.method)
  else if (request.method === "tools/call") {
    const connector = native.connectors[manifest.components.connectors[0].key]
    if (!connector.appTools.includes(request.params?.name))
      throw Error("Tool is not available to the panel")
    result = await client.call("tools/call", {
      name: request.params.name,
      arguments: request.params.arguments ?? {},
      _meta: { "iyw/conversationId": String(launch.conversationId) },
    })
  } else if (
    ![
      "ui/initialize",
      "ui/download-file",
      "ping",
      "ui/request-display-mode",
      "ui/notifications/sandbox-proxy-ready",
      "ui/notifications/initialized",
      "ui/notifications/size-changed",
      "ui/notifications/request-teardown",
    ].includes(request.method)
  ) {
    return {
      accepted: false,
      error: { code: -32601, message: "请在正式原助理会话中使用此操作" },
    }
  }
  return { accepted: true, result }
}

async function readBody(request) {
  const chunks = []
  let bytes = 0
  for await (const chunk of request) {
    bytes += chunk.length
    if (bytes > MAX_BODY_BYTES) throw Error("Preview request is too large")
    chunks.push(chunk)
  }
  return Buffer.concat(chunks).toString("utf8")
}

function send(response, type, body) {
  response.writeHead(200, {
    "Content-Type": `${type};charset=utf-8`,
    "Cache-Control": "no-store",
  })
  response.end(body)
}

async function close() {
  server.closeAllConnections()
  server.close()
  await client.close()
  if (
    path.dirname(profile) === data &&
    path.basename(profile).startsWith(".office-preview-")
  )
    await rm(profile, { recursive: true, force: true })
}
process.once("SIGINT", () => void close())
process.once("SIGTERM", () => void close())
