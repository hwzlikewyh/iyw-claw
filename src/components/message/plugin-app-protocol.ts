import { pluginAppMessage, type PluginAppResourceMeta } from "@/lib/api"
import { openUrl } from "@/lib/platform"
import type { PluginAppMessage } from "@/lib/plugin-app-bridge"
import type { MessageContext } from "./plugin-app-host"

type DisplayMode = "inline" | "fullscreen"

export function initializeResult(
  displayMode: DisplayMode,
  hostVersion: string,
  resourceMeta?: PluginAppResourceMeta
) {
  return {
    protocolVersion: "2026-01-26",
    hostCapabilities: {
      serverTools: {},
      serverResources: {},
      openLinks: {},
      message: { text: {}, image: {} },
      sandbox: {
        csp: resourceMeta?.csp,
        permissions: resourceMeta?.permissions,
      },
    },
    hostInfo: {
      name: "iyw-claw",
      version: hostVersion,
      client_type: "codex",
    },
    hostContext: {
      displayMode,
      availableDisplayModes: ["inline", "fullscreen"],
      containerDimensions: { maxHeight: 720, maxWidth: 1200 },
    },
  }
}

export function displayModeResult(context: MessageContext) {
  const mode = asRecord(context.message.params).mode
  const requested = mode === "fullscreen" || mode === "inline" ? mode : null
  return {
    mode: requested
      ? context.requestDisplayMode(requested)
      : context.currentDisplayMode(),
  }
}

export async function openLink(
  message: PluginAppMessage,
  authorized: Awaited<ReturnType<typeof pluginAppMessage>>
) {
  if (authorized.error) return authorized
  const url = asRecord(message.params).url
  if (typeof url !== "string" || !/^https?:\/\//i.test(url)) {
    return unsupported("Invalid URL", -32602)
  }
  await openUrl(url)
  return { accepted: true, result: {} }
}

export function resizeFrame(
  iframe: HTMLIFrameElement,
  message: PluginAppMessage
) {
  if (message.method !== "ui/notifications/size-changed") return
  const height = Number(asRecord(message.params).height)
  if (Number.isFinite(height) && height >= 120 && height <= 4096) {
    iframe.style.height = `${Math.round(height)}px`
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}
export function unsupported(message: string, code = -32601) {
  return { accepted: false, error: { code, message } }
}
