import { createHash } from "node:crypto"
import { existsSync, readFileSync, readdirSync } from "node:fs"
import { join, relative } from "node:path"

const HTTP_TIMEOUT_MS = 10_000
const HTTP_OK = 200
const HTTP_UNAUTHORIZED = 401
const SERVICE_POLL_MS = 500
const SERVICE_TIMEOUT_MS = 180_000

function fail(message) {
  throw new Error(message)
}

function expect(condition, message) {
  if (!condition) fail(message)
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex")
}

function walkFiles(root) {
  const files = []
  const stack = [root]
  while (stack.length > 0) {
    const directory = stack.pop()
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isDirectory()) stack.push(path)
      else if (entry.isFile()) files.push(path)
    }
  }
  return files
}

function assetTarget(options) {
  if (!existsSync(options.path)) {
    fail(`missing ${options.name} source asset: ${options.path}`)
  }
  return options
}

function pageTargets(root) {
  return [
    assetTarget({
      mimeTypes: ["text/html"],
      name: "root",
      path: join(root, "index.html"),
      route: "/",
    }),
    assetTarget({
      mimeTypes: ["text/html"],
      name: "extensionless-route",
      path: join(root, "settings", "appearance.html"),
      route: "/settings/appearance",
    }),
  ]
}

function discoverStaticTargets(root) {
  const files = walkFiles(root)
  const normalized = (path) => relative(root, path).replaceAll("\\", "/")
  const find = (predicate, label) => {
    const path = files.find((candidate) => predicate(normalized(candidate)))
    if (!path) fail(`missing ${label} asset below ${root}`)
    return path
  }
  const js = find(
    (path) => path.startsWith("_next/") && path.endsWith(".js"),
    "Next.js JavaScript"
  )
  const css = find(
    (path) => path.startsWith("_next/") && path.endsWith(".css"),
    "Next.js CSS"
  )
  const font = find(
    (path) => path.startsWith("_next/") && path.endsWith(".woff2"),
    "WOFF2 font"
  )
  const worker = find(
    (path) => /^vs\/assets\/[^/]*worker[^/]*\.js$/i.test(path),
    "Monaco worker"
  )
  const fileTarget = (name, path, mime) =>
    assetTarget({ mimeTypes: mime, name, path, route: `/${normalized(path)}` })
  return [
    ...pageTargets(root),
    fileTarget("next-javascript", js, [
      "text/javascript",
      "application/javascript",
    ]),
    fileTarget("next-css", css, ["text/css"]),
    fileTarget("woff2-font", font, ["font/woff2", "application/font-woff"]),
    fileTarget("monaco-worker", worker, [
      "text/javascript",
      "application/javascript",
    ]),
  ]
}

function requestBytes(url, options = {}) {
  return fetch(url, {
    method: options.method ?? "GET",
    headers: options.headers,
    redirect: "manual",
    signal: AbortSignal.timeout(HTTP_TIMEOUT_MS),
  }).then(async (response) => ({
    body: Buffer.from(await response.arrayBuffer()),
    headers: response.headers,
    status: response.status,
  }))
}

function assertHeaders(response, target, sourceLength) {
  const contentLength = Number.parseInt(
    response.headers.get("content-length") ?? "",
    10
  )
  const contentType = (response.headers.get("content-type") ?? "")
    .split(";", 1)[0]
    .toLowerCase()
  expect(
    contentLength === sourceLength,
    `${target.name}: invalid content length`
  )
  expect(
    target.mimeTypes.includes(contentType),
    `${target.name}: invalid MIME ${contentType}`
  )
  expect(
    response.headers.get("x-content-type-options") === "nosniff",
    `${target.name}: missing nosniff header`
  )
  return contentType
}

async function verifyStaticTarget(options) {
  const source = readFileSync(options.target.path)
  const url = `${options.baseUrl}${options.target.route}`
  const get = await requestBytes(url)
  expect(
    get.status === HTTP_OK,
    `${options.target.name}: GET returned ${get.status}`
  )
  const contentType = assertHeaders(get, options.target, source.length)
  expect(
    sha256(get.body) === sha256(source),
    `${options.target.name}: body hash mismatch`
  )
  const head = await requestBytes(url, { method: "HEAD" })
  expect(
    head.status === HTTP_OK,
    `${options.target.name}: HEAD returned ${head.status}`
  )
  expect(head.body.length === 0, `${options.target.name}: HEAD returned a body`)
  expect(
    assertHeaders(head, options.target, source.length) === contentType,
    `${options.target.name}: HEAD MIME mismatch`
  )
  return {
    bytes: source.length,
    contentType,
    route: options.target.route,
    sha256: sha256(source),
    source: relative(options.staticRoot, options.target.path).replaceAll(
      "\\",
      "/"
    ),
  }
}

async function verifyStaticAssets(options) {
  const results = []
  for (const target of discoverStaticTargets(options.staticRoot)) {
    results.push(await verifyStaticTarget({ ...options, target }))
  }
  return results
}

async function verifyApi(baseUrl, token) {
  const unauthorized = await requestBytes(`${baseUrl}/api/health`, {
    method: "POST",
  })
  expect(
    unauthorized.status === HTTP_UNAUTHORIZED,
    `health without token returned ${unauthorized.status}`
  )
  const authorized = await requestBytes(`${baseUrl}/api/health`, {
    headers: { authorization: `Bearer ${token}` },
    method: "POST",
  })
  expect(authorized.status === HTTP_OK, `health returned ${authorized.status}`)
  const payload = JSON.parse(authorized.body.toString("utf8"))
  expect(payload.status === "ok", "health status is not ok")
  expect(
    typeof payload.version === "string" && payload.version,
    "health version is missing"
  )
  return {
    status: payload.status,
    unauthorizedStatus: HTTP_UNAUTHORIZED,
    version: payload.version,
  }
}

function waitForWebSocket(baseUrl, token) {
  if (typeof WebSocket !== "function") fail("global WebSocket is unavailable")
  const wsUrl = baseUrl.replace(/^http/, "ws") + "/ws/events"
  const encoded = Buffer.from(token).toString("base64url")
  return new Promise((resolveReady, reject) => {
    const socket = new WebSocket(wsUrl, [
      "iyw-claw-events",
      `iyw-claw-token.${encoded}`,
    ])
    let settled = false
    let timeout
    const finish = (error, value) => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      try {
        socket.close()
      } catch {}
      if (error) reject(error)
      else resolveReady(value)
    }
    timeout = setTimeout(
      () => finish(new Error("WebSocket ready frame timed out")),
      HTTP_TIMEOUT_MS
    )
    socket.onerror = () => finish(new Error("WebSocket connection failed"))
    socket.onclose = () => {
      if (!settled) finish(new Error("WebSocket closed before ready frame"))
    }
    socket.onmessage = (event) => {
      try {
        const payload = JSON.parse(String(event.data))
        expect(
          payload.channel === "__ready__",
          "first WebSocket frame is not ready"
        )
        expect(payload.payload === null, "WebSocket ready payload is not null")
        expect(
          socket.protocol === "iyw-claw-events",
          `unexpected WebSocket protocol: ${socket.protocol}`
        )
        finish(null, { channel: payload.channel, protocol: socket.protocol })
      } catch (error) {
        finish(error)
      }
    }
  })
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds))
}

function assertProcessAlive(child) {
  if (child.spawnError)
    fail(`desktop launch failed: ${child.spawnError.message}`)
  if (child.exitCode !== null || child.signalCode !== null) {
    fail(`desktop exited before Web service startup: code=${child.exitCode}`)
  }
}

async function waitForService(baseUrl, token, child) {
  const deadline = Date.now() + SERVICE_TIMEOUT_MS
  let lastError = "no response"
  while (Date.now() < deadline) {
    assertProcessAlive(child)
    try {
      const response = await requestBytes(`${baseUrl}/api/health`, {
        headers: { authorization: `Bearer ${token}` },
        method: "POST",
      })
      if (response.status === HTTP_OK) return
      lastError = `HTTP ${response.status}`
    } catch (error) {
      lastError = error.message
    }
    await delay(SERVICE_POLL_MS)
  }
  fail(`Web service startup timed out: ${lastError}`)
}

export async function verifyInstalledWeb(options) {
  await waitForService(options.baseUrl, options.token, options.child)
  return {
    api: await verifyApi(options.baseUrl, options.token),
    staticAssets: await verifyStaticAssets(options),
    websocket: await waitForWebSocket(options.baseUrl, options.token),
  }
}
