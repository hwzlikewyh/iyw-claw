import { getTransport } from "@/lib/transport"
import {
  readPluginWindowContext,
  sendPluginWindowMessage,
  type PluginWindowContext,
} from "@/lib/plugin-window-context"
import type { PromptDraft, PromptInputBlock } from "@/lib/types"
import type { MessageContext } from "./plugin-app-messaging"
import type { PluginAppBridge } from "@/lib/plugin-app-bridge"

export const WINDOW_CONTEXT_TOOL = "infinite_canvas_host_context"
export const AUTHORIZE_WINDOW_MESSAGE = "ui/iyw/authorize-message"
const boundBridges = new WeakSet<PluginAppBridge>()

export function usesCurrentPluginWindow(context: MessageContext) {
  return boundBridges.has(context.bridge)
}

export function currentPluginWindow(context: MessageContext) {
  if (getTransport() !== context.transport)
    throw new Error("原助理连接已切换，请重新打开插件")
  const window = readPluginWindowContext(context.launch.conversationId)
  if (!window)
    throw new Error("插件尚未绑定当前原助理窗口，请在当前会话重新打开")
  return window
}

export function windowToolResult(context: MessageContext, result: unknown) {
  const value = result as {
    isError?: boolean
    content?: Array<{ type: string; text?: string }>
  }
  if (value?.isError) return result
  const text = value?.content?.find((entry) => entry.type === "text")?.text
  const authorized = text
    ? (JSON.parse(text) as { conversationId?: number; modelGateway?: boolean })
    : null
  if (
    authorized?.conversationId !== context.launch.conversationId ||
    !authorized.modelGateway
  )
    throw new Error("当前宿主未提供窗口模型桥接")
  const window = currentPluginWindow(context)
  boundBridges.add(context.bridge)
  return {
    content: [
      { type: "text", text: JSON.stringify({ ...window, modelGateway: true }) },
    ],
  }
}

export async function sendWindowMessage(
  context: MessageContext,
  expected: PluginWindowContext
) {
  currentPluginWindow(context)
  await sendPluginWindowMessage(expected, messageDraft(context.message.params))
  return { accepted: true, result: {} }
}

function messageDraft(params: unknown): PromptDraft {
  const value = params as { role?: unknown; content?: unknown }
  if (value?.role !== "user" || !Array.isArray(value.content))
    throw new Error("插件消息格式无效")
  const blocks: PromptInputBlock[] = value.content.map((part) => {
    if (
      part.type === "text" &&
      typeof part.text === "string" &&
      part.text.trim()
    )
      return { type: "text", text: part.text }
    if (
      part.type === "image" &&
      typeof part.data === "string" &&
      typeof part.mimeType === "string"
    )
      return { type: "image", data: part.data, mime_type: part.mimeType }
    throw new Error("插件消息包含不支持的内容")
  })
  if (!blocks.length) throw new Error("插件消息不能为空")
  return {
    blocks,
    displayText: blocks
      .flatMap((block) => (block.type === "text" ? [block.text] : []))
      .join("\n"),
  }
}
