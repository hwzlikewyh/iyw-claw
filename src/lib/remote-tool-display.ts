import type {
  AdaptedContentPart,
  AdaptedMessage,
  AdaptedToolCallPart,
} from "@/lib/adapters/ai-elements-adapter"
import {
  getIywCapabilityId,
  getRemoteToolDisplayTitle,
} from "@/lib/builtin-tool-display"

const MAX_CATALOG_DEPTH = 12
const DISCOVERY_TOOL_PATTERN =
  /(?:^|[./:_-])(?:search_iyw_capabilities|read_iyw_capability)$/i
const CATALOG_KEYS = [
  "structuredContent",
  "structured_content",
  "content",
  "text",
  "result",
  "capability",
  "capabilities",
  "items",
  "results",
  "Ok",
  "ok",
] as const
interface CatalogName {
  name: string
  rank: number
}

const toolMetadataCache = new WeakMap<
  AdaptedToolCallPart,
  {
    names: ReadonlyMap<string, CatalogName>
    capabilityId: string | null
    displayTitle: string | null
  }
>()
const displayPartCache = new WeakMap<AdaptedContentPart, AdaptedContentPart>()
const displayMessageCache = new WeakMap<AdaptedMessage, AdaptedMessage>()

function collectCatalogNames(
  value: unknown,
  names: Map<string, CatalogName>,
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
  names: Map<string, CatalogName>,
  depth: number
): void {
  if (record.isError === true || record.is_error === true) return
  const id = record.capability_id
  const annotations = record.annotations as Record<string, unknown> | undefined
  const schema = record.schema as
    | { annotations?: { title?: string }; title?: string }
    | undefined
  const candidates = [
    record.display_name,
    schema?.annotations?.title,
    annotations?.title,
    schema?.title,
    record.name,
  ]
  const index = candidates.findIndex(
    (value) => typeof value === "string" && !!value.trim()
  )
  const name = (candidates[index] as string | undefined)?.trim()
  const rank = index === candidates.length - 1 ? 1 : 2
  if (typeof id === "string" && id.startsWith("iyw.remote.") && name) {
    if (rank >= (names.get(id)?.rank ?? 0)) names.set(id, { name, rank })
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
    if (part.type === "goal-run") return toolCalls(part.items)
    return []
  })
}

function toolMetadata(part: AdaptedToolCallPart) {
  const cached = toolMetadataCache.get(part)
  if (cached) return cached
  const names = new Map<string, CatalogName>()
  const discovery = DISCOVERY_TOOL_PATTERN.test(part.toolName)
  if (discovery && part.state !== "output-error" && !part.errorText) {
    collectCatalogNames(part.output, names)
  }
  const capabilityId = getIywCapabilityId(part.input)
  const displayTitle = getRemoteToolDisplayTitle(part.input, part.meta)
  const metadata = { names, capabilityId, displayTitle }
  toolMetadataCache.set(part, metadata)
  return metadata
}

function nameToolCall(
  part: AdaptedToolCallPart,
  names: ReadonlyMap<string, CatalogName>
): AdaptedToolCallPart {
  const metadata = toolMetadata(part)
  const id = metadata.capabilityId
  const title = metadata.displayTitle ?? (id ? names.get(id)?.name : undefined)
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
  names: ReadonlyMap<string, CatalogName>
): AdaptedContentPart {
  if (part.type === "tool-call") return nameToolCall(part, names)
  if (part.type === "goal-run") {
    const items = part.items.map((item) => namePart(item, names))
    if (items.every((item, index) => item === part.items[index])) return part
    const cached = displayPartCache.get(part)
    if (
      cached?.type === "goal-run" &&
      items.every((item, index) => item === cached.items[index])
    ) {
      return cached
    }
    const named = { ...part, items }
    displayPartCache.set(part, named)
    return named
  }
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
  names: ReadonlyMap<string, CatalogName>
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
  const names = new Map<string, CatalogName>()
  let hasDisplayTitle = false
  for (const message of messages) {
    if (message.role !== "assistant") continue
    for (const part of toolCalls(message.content)) {
      const metadata = toolMetadata(part)
      metadata.names.forEach((entry, id) => {
        if (entry.rank >= (names.get(id)?.rank ?? 0)) names.set(id, entry)
      })
      hasDisplayTitle ||= metadata.displayTitle !== null
    }
  }
  if (names.size === 0 && !hasDisplayTitle) return messages
  return messages.map((message) =>
    message.role === "assistant" ? nameMessage(message, names) : message
  )
}
