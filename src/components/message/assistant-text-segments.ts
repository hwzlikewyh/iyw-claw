import { fromMarkdown } from "mdast-util-from-markdown"

const TEXT_SEGMENT_LENGTH = 900

// 只在完整 Markdown 块后分段，代码、列表、表格及交互内容保持完整。
export function splitAssistantText(text: string): string[] {
  if (text.length <= TEXT_SEGMENT_LENGTH || /<|:::/u.test(text)) return [text]
  const nodes = fromMarkdown(text).children
  if (nodes.some((node) => node.type === "definition")) return [text]
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
  return segments
}
