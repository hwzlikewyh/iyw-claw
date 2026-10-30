import { getTransport } from "@/lib/transport"
import type { ShareLocation, ShareRecord } from "./types"

export function shareAction<T>(action: string, data: unknown): Promise<T> {
  return getTransport().call("conversation_share_action", { action, data })
}

export function validateShareBase(raw: string): string {
  const url = new URL(raw)
  const local = ["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)
  if (
    (url.protocol !== "https:" && !(local && url.protocol === "http:")) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  )
    throw new Error("分享服务地址无效")
  return url.toString().replace(/\/$/, "")
}

export function shareFileURL(
  location: ShareLocation,
  reference: string,
  download = false
) {
  const params = new URLSearchParams({ token: location.token, reference })
  if (download) params.set("download", "1")
  return `${validateShareBase(location.apiBase)}/conversation-shares/file?${params}`
}

export function sharePageURL(location: ShareLocation, webBase: string) {
  const base = validateShareBase(webBase)
  const root = `${base.replace(/\/+$/, "")}/`
  const url = new URL(base)
  if (
    !/\.html$/i.test(url.pathname) &&
    !/\/conversation-shares\/view\/?$/i.test(url.pathname)
  ) {
    return buildSharePageURL(location, new URL("share/", root))
  }
  return buildSharePageURL(location, url)
}

function buildSharePageURL(location: ShareLocation, url: URL) {
  // 凭据留在 fragment，不进入网页服务器请求和 Referer。
  url.hash = new URLSearchParams({
    token: location.token,
    api: location.apiBase,
  }).toString()
  return url.toString()
}

export function parseShareLocation(): ShareLocation {
  const params = new URLSearchParams(window.location.hash.slice(1))
  const token = params.get("token") ?? ""
  if (!/^[a-f0-9]{64}$/.test(token)) throw new Error("分享链接无效")
  return {
    token,
    apiBase: validateShareBase(params.get("api") ?? window.location.origin),
  }
}

export async function readShare(
  location: ShareLocation,
  signal?: AbortSignal
): Promise<ShareRecord> {
  const url = new URL(
    `${validateShareBase(location.apiBase)}/conversation-shares/read`
  )
  url.searchParams.set("token", location.token)
  const controller = new AbortController()
  const abort = () => controller.abort()
  if (signal?.aborted) abort()
  signal?.addEventListener("abort", abort, { once: true })
  const timeout = setTimeout(abort, 15_000)
  try {
    const response = await fetch(url, {
      signal: controller.signal,
      cache: "no-store",
      referrerPolicy: "no-referrer",
    })
    if (!response.ok) throw new Error("分享暂时无法访问，请重试")
    const envelope = await response.json()
    if (envelope.code !== 1) throw new Error(envelope.message || "分享已失效")
    return envelope.data as ShareRecord
  } finally {
    clearTimeout(timeout)
    signal?.removeEventListener("abort", abort)
  }
}
