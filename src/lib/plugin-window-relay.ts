import { getTransport } from "./transport"
import { toErrorMessage } from "./app-error"
import {
  readPluginWindowContext,
  sendPluginWindowMessage,
  onPluginWindowContextChanged,
  registerDetachedPluginContext,
  notifyPluginWindowContext,
  type PluginWindowContext,
} from "./plugin-window-context"
import type { PromptDraft } from "./types"

const HEARTBEAT_MS = 5_000
const REQUEST_TIMEOUT_MS = 30_000
const RELAY_TTL_MS = 60_000
type RelayMessage =
  | { kind: "read" | "close" }
  | { kind: "context"; value: PluginWindowContext | null }
  | {
      kind: "send"
      id: string
      expected: PluginWindowContext
      draft: PromptDraft
    }
  | { kind: "result"; id: string; error?: string }

export function createPluginWindowRelay(conversationId: number) {
  const id = crypto.randomUUID()
  const channel = new BroadcastChannel(`plugin-window:${id}`)
  const transport = getTransport()
  let touched = Date.now()
  let disposed = false
  const publish = () =>
    channel.postMessage({
      kind: "context",
      value:
        transport === getTransport()
          ? readPluginWindowContext(conversationId)
          : null,
    })
  const unsubscribe = onPluginWindowContextChanged(publish)
  const timer = setInterval(() => {
    if (Date.now() - touched > RELAY_TTL_MS) dispose()
  }, HEARTBEAT_MS)
  const dispose = () => {
    if (disposed) return
    disposed = true
    clearInterval(timer)
    unsubscribe()
    channel.close()
  }
  channel.onmessage = ({ data }: MessageEvent<RelayMessage>) => {
    touched = Date.now()
    if (data.kind === "close") return dispose()
    if (data.kind === "read") return publish()
    if (data.kind !== "send") return
    void (async () => {
      try {
        if (
          getTransport() !== transport ||
          data.expected.conversationId !== conversationId
        )
          throw Error("原对话连接已变化，请重新打开插件")
        await sendPluginWindowMessage(data.expected, data.draft)
        if (!disposed) channel.postMessage({ kind: "result", id: data.id })
      } catch (cause) {
        if (disposed) return
        channel.postMessage({
          kind: "result",
          id: data.id,
          error: toErrorMessage(cause),
        })
      }
    })()
  }
  return { id, dispose }
}

export function connectPluginWindowRelay(
  id: string,
  conversationId: number,
  ready: () => void
) {
  const channel = new BroadcastChannel(`plugin-window:${id}`)
  let context: PluginWindowContext | null = null
  let received = 0
  const pending = new Map<string, (error?: string) => void>()
  const unregister = registerDetachedPluginContext({
    context: () => (Date.now() - received < RELAY_TTL_MS ? context : null),
    send: (expected, draft) =>
      requestSend(channel, pending, { expected, draft }),
  })
  channel.onmessage = ({ data }: MessageEvent<RelayMessage>) => {
    if (data.kind === "context") {
      const next =
        data.value?.conversationId === conversationId ? data.value : null
      const changed = JSON.stringify(next) !== JSON.stringify(context)
      context = next
      received = Date.now()
      if (changed) notifyPluginWindowContext()
      ready()
    } else if (data.kind === "result") pending.get(data.id)?.(data.error)
  }
  const read = () => channel.postMessage({ kind: "read" })
  read()
  const timer = setInterval(read, HEARTBEAT_MS)
  return () => {
    clearInterval(timer)
    unregister()
    for (const finish of pending.values()) finish("插件窗口已关闭")
    // StrictMode 的立即重挂载复用该通道，不发送关闭通知中断来源。
    channel.close()
  }
}

function requestSend(
  channel: BroadcastChannel,
  pending: Map<string, (error?: string) => void>,
  input: {
    expected: PluginWindowContext
    draft: PromptDraft
  }
) {
  return new Promise<void>((resolve, reject) => {
    const id = crypto.randomUUID()
    const timer = setTimeout(
      () => finish("原对话暂未响应，请查看原窗口"),
      REQUEST_TIMEOUT_MS
    )
    const finish = (error?: string) => {
      clearTimeout(timer)
      pending.delete(id)
      if (error) reject(Error(error))
      else resolve()
    }
    pending.set(id, finish)
    channel.postMessage({ kind: "send", id, ...input })
  })
}
