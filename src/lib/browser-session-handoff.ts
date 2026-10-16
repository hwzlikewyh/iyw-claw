import { getCurrentWindow, isDesktop } from "@/lib/platform"
import { getShellTransport } from "@/lib/transport"
import { useTabStore } from "@/stores/tab-store"
import { randomUUID } from "@/lib/utils"
import type { ReferenceAttrs } from "@/components/chat/composer/types"
import {
  emitAppendTextToSession,
  emitAttachImageToSession,
  emitInsertReferenceToSession,
  type AttachImageToSessionDetail,
} from "@/lib/session-attachment-events"

export const BROWSER_SESSION_REQUEST = "browser://session-request"
export const BROWSER_SESSION_RESPONSE = "browser://session-response"
const HANDOFF_TIMEOUT_MS = 5_000
const COMPOSER_READY_ATTEMPTS = 40
const COMPOSER_READY_INTERVAL_MS = 25

export type BrowserSessionContent =
  | { kind: "text"; text: string }
  | { kind: "reference"; reference: ReferenceAttrs }
  | { kind: "image"; image: Omit<AttachImageToSessionDetail, "tabId"> }

type BrowserSessionOperation =
  | { kind: "target" }
  | { kind: "deliver"; target: string; content: BrowserSessionContent }

export type BrowserSessionRequest = BrowserSessionOperation & {
  requestId: string
  sourceWindow: string
}

interface BrowserSessionResponse {
  requestId: string
  target: string | null
  accepted: boolean
}

export function resolveMainBrowserSession(): string {
  const store = useTabStore.getState()
  if (!store.rawTabs.some((tab) => tab.id === store.activeTabId))
    store.openChatModeTab()
  const target = useTabStore.getState().activeTabId
  if (!target) throw new Error("The conversation input is unavailable")
  return target
}

function dispatchContent(target: string, content: BrowserSessionContent) {
  switch (content.kind) {
    case "text":
      return emitAppendTextToSession({ tabId: target, text: content.text })
    case "reference":
      return emitInsertReferenceToSession({
        tabId: target,
        reference: content.reference,
      })
    case "image":
      return emitAttachImageToSession({ tabId: target, ...content.image })
  }
}

export async function deliverMainBrowserSession(
  target: string,
  content: BrowserSessionContent
): Promise<boolean> {
  for (let attempt = 0; attempt < COMPOSER_READY_ATTEMPTS; attempt += 1) {
    if (!useTabStore.getState().rawTabs.some((tab) => tab.id === target)) {
      console.warn("[Browser] target conversation has closed", {
        target,
        kind: content.kind,
      })
      return false
    }
    if (dispatchContent(target, content)) {
      console.info("[Browser] content added to conversation", {
        target,
        kind: content.kind,
      })
      return true
    }
    await new Promise((resolve) =>
      setTimeout(resolve, COMPOSER_READY_INTERVAL_MS)
    )
  }
  console.warn("[Browser] conversation handoff was not accepted", {
    kind: content.kind,
  })
  return false
}

function waitForResponse(response: Promise<BrowserSessionResponse>) {
  let timer: ReturnType<typeof setTimeout> | undefined
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(
      () => reject(new Error("Browser conversation handoff timed out")),
      HANDOFF_TIMEOUT_MS
    )
  })
  return Promise.race([response, timeout]).finally(() => clearTimeout(timer))
}

async function requestMainSession(operation: BrowserSessionOperation) {
  const current = await getCurrentWindow()
  if (!current) throw new Error("The browser window is unavailable")
  const requestId = randomUUID()
  let complete!: (result: BrowserSessionResponse) => void
  const response = new Promise<BrowserSessionResponse>((resolve) => {
    complete = resolve
  })
  const stop = await getShellTransport().subscribe<BrowserSessionResponse>(
    BROWSER_SESSION_RESPONSE,
    (result) => {
      if (result.requestId === requestId) complete(result)
    }
  )
  try {
    const { emitTo } = await import("@tauri-apps/api/event")
    await emitTo("main", BROWSER_SESSION_REQUEST, {
      ...operation,
      requestId,
      sourceWindow: current.label,
    })
    return await waitForResponse(response)
  } finally {
    stop()
  }
}

export async function getBrowserSessionTarget(): Promise<string> {
  const current = isDesktop() ? await getCurrentWindow() : null
  if (!current || current.label === "main") return resolveMainBrowserSession()
  const result = await requestMainSession({ kind: "target" })
  if (!result.target) throw new Error("The conversation input is unavailable")
  return result.target
}

export async function deliverBrowserSession(
  target: string,
  content: BrowserSessionContent
): Promise<boolean> {
  const current = isDesktop() ? await getCurrentWindow() : null
  if (!current || current.label === "main")
    return deliverMainBrowserSession(target, content)
  const result = await requestMainSession({ kind: "deliver", target, content })
  return result.accepted
}

export async function handleBrowserSessionRequest(
  request: BrowserSessionRequest
) {
  let target: string | null = null
  let accepted = false
  try {
    target =
      request.kind === "target" ? resolveMainBrowserSession() : request.target
    accepted =
      request.kind === "target" ||
      (await deliverMainBrowserSession(target, request.content))
  } catch (error) {
    console.warn("[Browser] conversation handoff failed", {
      kind: request.kind,
      error,
    })
  }
  const { emitTo } = await import("@tauri-apps/api/event")
  await emitTo(request.sourceWindow, BROWSER_SESSION_RESPONSE, {
    requestId: request.requestId,
    target,
    accepted,
  })
}
