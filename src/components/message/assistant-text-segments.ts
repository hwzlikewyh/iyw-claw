import { fromMarkdown } from "mdast-util-from-markdown"

const TEXT_SEGMENT_LENGTH = 900
const TEXT_SEGMENT_CACHE_LIMIT = 24
const segmentCache = new Map<string, string[]>()

function rememberSegments(text: string, segments: string[]): string[] {
  segmentCache.set(text, segments)
  if (segmentCache.size > TEXT_SEGMENT_CACHE_LIMIT) {
    const oldest = segmentCache.keys().next().value
    if (oldest !== undefined) segmentCache.delete(oldest)
  }
  return segments
}

// 只在完整 Markdown 块后分段，代码、列表、表格及交互内容保持完整。
export function splitAssistantText(text: string): string[] {
  if (text.length <= TEXT_SEGMENT_LENGTH || /<|:::/u.test(text)) return [text]
  // 已结束的正文在同轮后续输出中保持不变，复用分段并限制缓存数量。
  const cached = segmentCache.get(text)
  if (cached) return cached
  const nodes = fromMarkdown(text).children
  if (nodes.some((node) => node.type === "definition"))
    return rememberSegments(text, [text])
  const segments: string[] = []
  let start = 0
  for (const node of nodes.slice(0, -1)) {
    const end = node.position?.end.offset
    if (!end || end - start < TEXT_SEGMENT_LENGTH || node.type === "heading")
      continue
    segments.push(text.slice(start, end))
    start = end
  }
  segments.push(text.slice(start))
  return rememberSegments(text, segments)
}

function collectMarkdownText(node: unknown, output: string[]) {
  if (!node || typeof node !== "object") return
  const record = node as Record<string, unknown>
  if (record.type === "text" || record.type === "inlineCode") {
    output.push(typeof record.value === "string" ? record.value : "")
    return
  }
  if (record.type === "image" || record.type === "imageReference") {
    output.push(typeof record.alt === "string" ? record.alt : "")
    return
  }
  if (Array.isArray(record.children))
    record.children.forEach((child) => collectMarkdownText(child, output))
}

export function assistantResponseFirstLine(parts: string[]): string {
  const text = parts.join("\n\n").trim()
  const first = fromMarkdown(text).children[0]
  if (!first) return text.replace(/\s+/g, " ")
  const output: string[] = []
  collectMarkdownText(first, output)
  return (output.join("").trim() || text).replace(/\s+/g, " ")
}
