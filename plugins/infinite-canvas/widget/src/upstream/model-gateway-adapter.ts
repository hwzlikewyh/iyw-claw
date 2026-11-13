import axios, { AxiosError, AxiosHeaders, type AxiosRequestConfig } from "axios"
import type { App } from "@modelcontextprotocol/ext-apps"
import { runLegacyModel } from "./legacy-model.js"

const GATEWAY_ORIGIN = "http://iyw-canvas.local"
const MODEL_REQUEST_TIMEOUT_MS = 900_000
const allowedPaths = new Set(["models", "chat/completions", "responses", "responses/compact", "images/generations", "images/edits"])

export type GatewayResult = { status: number; contentType: string; body: { text?: string; base64?: string } }
type CanvasHost = Pick<App, "callServerTool" | "sendMessage">
type GatewayRequest = { pathname: string; method: string; body: unknown; query: Record<string, string>; signal?: { readonly aborted: boolean } }

export function installModelGatewayAdapter(app: CanvasHost): void {
  const fallback = axios.getAdapter(axios.defaults.adapter)
  axios.defaults.adapter = async (config) => {
    const url = resolveUrl(config)
    if (!url || url.origin !== GATEWAY_ORIGIN) return fallback(config)
    const result = await request(app, { url, method: config.method || "get", rawBody: config.data, signal: config.signal })
    const data = decodeResponse(result, config.responseType)
    const response = { data, status: result.status, statusText: statusText(result.status), headers: new AxiosHeaders({ "content-type": result.contentType }), config, request: {} }
    const validate = config.validateStatus || ((status: number) => status >= 200 && status < 300)
    if (!validate(result.status)) throw new AxiosError(`Request failed with status code ${result.status}`, undefined, config, {}, response)
    return response
  }
  installFetchAdapter(app)
}

function installFetchAdapter(app: CanvasHost): void {
  const original = window.fetch.bind(window)
  window.fetch = async (input, init) => {
    const request = input instanceof Request ? input : undefined
    const url = new URL(request ? request.url : String(input), window.location.href)
    if (url.origin !== GATEWAY_ORIGIN) return original(input, init)
    const method = (init?.method || request?.method || "GET").toUpperCase()
    const body = await readFetchBody(request, init)
    const query = Object.fromEntries(url.searchParams.entries())
    const result = await callGateway(app, { pathname: url.pathname, method, body, query, signal: init?.signal ?? request?.signal })
    return toFetchResponse(result)
  }
}

async function readFetchBody(request: Request | undefined, init?: RequestInit): Promise<unknown> {
  const body = init?.body ?? (request ? await request.clone().text() : undefined)
  if (typeof body !== "string" || !body) return undefined
  try { return JSON.parse(body) } catch { throw new Error("Local gateway requests must use JSON") }
}

function resolveUrl(config: AxiosRequestConfig): URL | undefined {
  try { return new URL(axios.getUri(config), window.location.href) } catch { return undefined }
}

async function request(app: CanvasHost, input: { url: URL; method: string; rawBody: unknown; signal?: { readonly aborted: boolean } }): Promise<GatewayResult> {
  const { url, method, rawBody, signal } = input
  if (signal?.aborted) throw new DOMException("Aborted", "AbortError")
  const path = normalizePath(url.pathname)
  if (path.startsWith("videos") || path === "audio/speech") {
    throw new Error(navigator.language.startsWith("zh") ? "iyw-claw 本地网关暂未提供视频或音频生成接口" : "Video and audio generation are not available through the iyw-claw local gateway yet")
  }
  if (!allowedPaths.has(path)) throw new Error("This local gateway endpoint is not supported")
  const body = await normalizeBody(path, rawBody)
  const result = await callGateway(app, { pathname: url.pathname, method: method.toUpperCase(), body, query: Object.fromEntries(url.searchParams.entries()), signal })
  if (signal?.aborted) throw new DOMException("Aborted", "AbortError")
  if (result.status < 200 || result.status >= 300) throw new Error(errorMessage(result))
  return result
}

async function normalizeBody(path: string, value: unknown): Promise<unknown> {
  if (!(value instanceof FormData)) return parseJsonBody(value)
  const body: Record<string, unknown> = {}
  const images: Array<{ image_url: string }> = []
  for (const [key, item] of value.entries()) {
    if (typeof item === "string") {
      body[key] = item
      continue
    }
    const image = await blobDataUrl(item)
    if (key === "mask") body.mask = { image_url: image }
    else if (key === "image" || key === "image[]") images.push({ image_url: image })
    else throw new Error("This local gateway does not support this multipart field")
  }
  if (path === "images/edits" && images.length) body.images = images
  return body
}

function parseJsonBody(value: unknown): unknown {
  if (typeof value !== "string") return value
  try { return JSON.parse(value) } catch { throw new Error("Local gateway requests must use JSON") }
}

async function blobDataUrl(blob: Blob): Promise<string> {
  const bytes = new Uint8Array(await blob.arrayBuffer())
  let binary = ""
  for (let index = 0; index < bytes.length; index += 0x8000) binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000))
  return `data:${blob.type || "application/octet-stream"};base64,${btoa(binary)}`
}

async function callGateway(app: CanvasHost, request: GatewayRequest): Promise<GatewayResult> {
  const { pathname, method, body, query, signal } = request
  const path = normalizePath(pathname)
  if (!allowedPaths.has(path)) throw new Error("This local gateway endpoint is not supported")
  if (path === "models" ? method !== "GET" : method !== "POST") throw new Error("This local gateway method is not supported")
  const response = await app.callServerTool({
    name: "infinite_canvas_model_request",
    arguments: { path, method, ...(body === undefined ? {} : { body }), ...(Object.keys(query).length ? { query } : {}) },
  }, { timeout: MODEL_REQUEST_TIMEOUT_MS })
  const text = response.content.find((item) => item.type === "text")?.text
  if (!text) throw new Error("The local model gateway is unavailable")
  if (response.isError) {
    let message = text
    try { message = (JSON.parse(text) as { message?: string }).message || text } catch { /* 保留宿主的原始错误 */ }
    throw new Error(message)
  }
  const result = JSON.parse(text) as GatewayResult & { delegated?: boolean; requestId?: string }
  if (result.delegated && result.requestId) return runLegacyModel(app, result.requestId, { path, body, signal })
  return result
}

function decodeResponse(result: GatewayResult, responseType?: string): unknown {
  if (result.body.text !== undefined) {
    if (responseType === "text") return result.body.text
    if (responseType === "blob") return new Blob([result.body.text], { type: result.contentType })
    if (responseType === "arraybuffer") return new TextEncoder().encode(result.body.text).buffer
    try { return JSON.parse(result.body.text) } catch { return result.body.text }
  }
  const bytes = Uint8Array.from(atob(result.body.base64 || ""), (item) => item.charCodeAt(0))
  if (responseType === "text") return new TextDecoder().decode(bytes)
  if (responseType === "arraybuffer") return bytes.buffer
  return new Blob([bytes], { type: result.contentType })
}

function toFetchResponse(result: GatewayResult): Response {
  const text = result.body.text ?? atob(result.body.base64 || "")
  const body = result.body.text !== undefined ? text : Uint8Array.from(text, (item) => item.charCodeAt(0))
  return new Response(body, { status: result.status, headers: { "content-type": result.contentType } })
}

function normalizePath(pathname: string): string {
  return pathname.replace(/^\/+/, "").replace(/^v1\//, "")
}

function errorMessage(result: GatewayResult): string {
  if (result.body.text) {
    try {
      const value = JSON.parse(result.body.text) as { message?: string; error?: { message?: string } }
      return value.error?.message || value.message || `Local gateway returned HTTP ${result.status}`
    } catch { return `Local gateway returned HTTP ${result.status}` }
  }
  return `Local gateway returned HTTP ${result.status}`
}

function statusText(status: number): string {
  return status >= 200 && status < 300 ? "OK" : "Gateway Error"
}
