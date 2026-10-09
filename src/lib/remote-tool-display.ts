import type {
  AdaptedContentPart,
  AdaptedMessage,
  AdaptedToolCallPart,
} from "@/lib/adapters/ai-elements-adapter"
import { getIywCapabilityId } from "@/lib/builtin-tool-display"

const MAX_CATALOG_DEPTH = 12
const DISCOVERY_TOOL_PATTERN =
  /(?:^|[./:_-])(?:search_iyw_capabilities|read_iyw_capability)$/i
const INVOKE_TOOL_PATTERN = /(?:^|[./:_-])invoke_iyw_capability$/i
const CATALOG_KEYS = [
  "structuredContent",
  "structured_content",
  "content",
  "text",
  "result",
  "capability",
  "capabilities",
  "items",
] as const
const toolMetadataCache = new WeakMap<
  AdaptedToolCallPart,
  { names: ReadonlyMap<string, string>; capabilityId: string | null }
>()
const displayPartCache = new WeakMap<AdaptedContentPart, AdaptedContentPart>()
const displayMessageCache = new WeakMap<AdaptedMessage, AdaptedMessage>()

function collectCatalogNames(
  value: unknown,
  names: Map<string, string>,
  depth = 0
): void {
  if (value == null || depth > MAX_CATALOG_DEPTH) return
  if (typeof value === "string") {
    try {
      collectCatalogNames(JSON.parse(value), names, depth + 1)
    } catch {
      // Partial or non-JSON output does not establish a tool name.
    }
    return
  }
  if (Array.isArray(value)) {
    value.forEach((item) => collectCatalogNames(item, names, depth + 1))
    return
  }
  if (typeof value !== "object") return
  collectCatalogRecord(value as Record<string, unknown>, names, depth)
}

function collectCatalogRecord(
  record: Record<string, unknown>,
  names: Map<string, string>,
  depth: number
): void {
  if (record.isError === true || record.is_error === true) return
  const id = record.capability_id
  const name = typeof record.name === "string" ? record.name.trim() : ""
  if (typeof id === "string" && id.startsWith("iyw.remote.") && name) {
    names.set(id, name)
  }
  CATALOG_KEYS.forEach((key) =>
    collectCatalogNames(record[key], names, depth + 1)
  )
}

function toolCalls(
  parts: readonly AdaptedContentPart[]
): AdaptedToolCallPart[] {
  return parts.flatMap((part) => {
    if (part.type === "tool-call") return [part]
    if (part.type === "tool-group") return part.items
    return []
  })
}

function toolMetadata(part: AdaptedToolCallPart) {
  const cached = toolMetadataCache.get(part)
  if (cached) return cached
  const names = new Map<string, string>()
  const discovery = DISCOVERY_TOOL_PATTERN.test(part.toolName)
  if (discovery && part.state !== "output-error" && !part.errorText) {
    collectCatalogNames(part.output, names)
  }
  const capabilityId =
    discovery || INVOKE_TOOL_PATTERN.test(part.toolName)
      ? getIywCapabilityId(part.input)
      : null
  const metadata = { names, capabilityId }
  toolMetadataCache.set(part, metadata)
  return metadata
}

function nameToolCall(
  part: AdaptedToolCallPart,
  names: ReadonlyMap<string, string>
): AdaptedToolCallPart {
  const id = toolMetadata(part).capabilityId
  const title = id ? names.get(id) : undefined
  if (!title || part.displayTitle === title) return part
  const cached = displayPartCache.get(part)
  if (cached?.type === "tool-call" && cached.displayTitle === title)
    return cached
  const named = { ...part, displayTitle: title }
  displayPartCache.set(part, named)
  return named
}

function namePart(
  part: AdaptedContentPart,
  names: ReadonlyMap<string, string>
): AdaptedContentPart {
  if (part.type === "tool-call") return nameToolCall(part, names)
  if (part.type !== "tool-group") return part
  const items = part.items.map((item) => nameToolCall(item, names))
  if (items.every((item, index) => item === part.items[index])) return part
  const cached = displayPartCache.get(part)
  if (
    cached?.type === "tool-group" &&
    items.every((item, index) => item === cached.items[index])
  ) {
    return cached
  }
  const named = { ...part, items }
  displayPartCache.set(part, named)
  return named
}

function nameMessage(
  message: AdaptedMessage,
  names: ReadonlyMap<string, string>
): AdaptedMessage {
  const content = message.content.map((part) => namePart(part, names))
  if (content.every((part, index) => part === message.content[index]))
    return message
  const cached = displayMessageCache.get(message)
  if (
    cached &&
    content.every((part, index) => part === cached.content[index])
  ) {
    return cached
  }
  const named = { ...message, content }
  displayMessageCache.set(message, named)
  return named
}

export function resolveRemoteToolDisplayNames(
  messages: AdaptedMessage[]
): AdaptedMessage[] {
  const names = new Map<string, string>()
  for (const message of messages) {
    if (message.role !== "assistant") continue
    for (const part of toolCalls(message.content)) {
      toolMetadata(part).names.forEach((name, id) => names.set(id, name))
    }
  }
  if (names.size === 0) return messages
  return messages.map((message) =>
    message.role === "assistant" ? nameMessage(message, names) : message
  )
}
