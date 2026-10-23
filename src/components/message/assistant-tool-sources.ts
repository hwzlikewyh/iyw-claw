import type { AdaptedToolCallPart } from "@/lib/adapters/ai-elements-adapter"
import { normalizeToolName } from "@/lib/tool-call-normalization"
import { processToolInput } from "./assistant-tool-presentation"

interface ToolSource {
  title: string
  url: string
}
const MAX_SOURCES = 10
const MAX_DEPTH = 4
const MAX_EXCERPT = 4000
const SOURCE_FIELDS = ["sources", "results", "links", "content", "text", "data"]

function source(title: unknown, address: unknown): ToolSource[] {
  if (typeof address !== "string") return []
  try {
    const url = new URL(address)
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password
    )
      return []
    return [
      {
        title:
          typeof title === "string" && title.trim()
            ? title.trim()
            : url.hostname,
        url: url.href,
      },
    ]
  } catch {
    return []
  }
}

function decodeOutput(value: string): unknown {
  try {
    return JSON.parse(value) as unknown
  } catch {
    return value
  }
}

function collectSources(value: unknown, depth = 0): ToolSource[] {
  if (depth > MAX_DEPTH || !value) return []
  if (Array.isArray(value))
    return value.flatMap((item) => collectSources(item, depth + 1))
  if (typeof value === "string") {
    return Array.from(
      value.matchAll(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g)
    ).flatMap((match) => source(match[1], match[2]))
  }
  if (typeof value !== "object") return []
  const object = value as Record<string, unknown>
  return [
    ...source(
      object.title ?? object.name,
      object.url ?? object.href ?? object.link
    ),
    ...SOURCE_FIELDS.flatMap((key) => collectSources(object[key], depth + 1)),
  ]
}

function outputText(value: unknown, depth = 0): string {
  if (depth > MAX_DEPTH || !value) return ""
  if (typeof value === "string") return value
  if (Array.isArray(value))
    return value.map((item) => outputText(item, depth + 1)).join("\n\n")
  if (typeof value !== "object") return ""
  const object = value as Record<string, unknown>
  return outputText(object.text ?? object.content, depth + 1)
}

export function processToolDetails(tool: AdaptedToolCallPart) {
  const name = normalizeToolName(tool.toolName).toLowerCase()
  if (
    !["websearch", "webfetch"].includes(name) ||
    tool.state === "output-error"
  )
    return null
  const output = decodeOutput(tool.output ?? "")
  const sources = collectSources(output)
  if (name === "webfetch") {
    const input = processToolInput(tool.input)
    sources.unshift(...source(null, input.url))
  }
  const seen = new Set<string>()
  const unique = sources
    .filter((item) => {
      if (seen.has(item.url)) return false
      seen.add(item.url)
      return true
    })
    .slice(0, MAX_SOURCES)
  const text =
    name === "webfetch" ? outputText(output).slice(0, MAX_EXCERPT) : ""
  return unique.length || text ? { sources: unique, text } : null
}
