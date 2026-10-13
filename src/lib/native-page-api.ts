import { getShellTransport } from "@/lib/transport"
import type { PageCapture } from "@/lib/page-capture"

export type PageBounds = { x: number; y: number; width: number; height: number }
export type NativePageState = {
  id: string
  url: string
  title: string
  loading: boolean
  canBack: boolean
  canForward: boolean
  errorCount: number
}
export type NativePageEvent = {
  id: string
  type: "state" | "picked" | "error" | "new-tab" | "download" | "shortcut"
  state?: NativePageState
  url?: string
  title?: string
  tag?: string
  selector?: string
  text?: string
  message?: string
  name?: string
  cancelled?: boolean
  path?: string | null
  status?: "started" | "completed" | "failed"
}

export type NativePageDownload = Pick<
  NativePageEvent,
  "url" | "name" | "path" | "status"
>

export const NATIVE_PAGE_EVENT = "interactive-page://event"
export function nativePageCommand<T = NativePageState>(
  id: string,
  action: string,
  options: Record<string, unknown> = {}
): Promise<T> {
  return getShellTransport().call("native_page_command", {
    request: { id, action, ...options },
  })
}

export function captureNativePage(id: string): Promise<PageCapture> {
  return nativePageCommand<PageCapture>(id, "capture")
}

export function normalizePageUrl(value: string): string | null {
  const input = value.trim()
  if (!input) return null
  try {
    const url = new URL(
      /^[a-z][a-z0-9+.-]*:/i.test(input) &&
        !/^(localhost|[a-z0-9.-]+\.\w+):\d+(?:\/|$)/i.test(input)
        ? input
        : `${/^(localhost|127\.|\[::1\])(?::|\/|$)/i.test(input) ? "http" : "https"}://${input}`
    )
    return ["https:", "http:"].includes(url.protocol) &&
      !url.username &&
      !url.password
      ? url.href
      : null
  } catch {
    return null
  }
}
