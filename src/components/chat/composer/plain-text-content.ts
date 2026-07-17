import type { JSONContent } from "@tiptap/core"

export function textToInlineContent(text: string): JSONContent[] {
  if (!text) return []
  const content: JSONContent[] = []
  text.split("\n").forEach((line, index) => {
    if (index > 0) content.push({ type: "hardBreak" })
    if (line.length > 0) content.push({ type: "text", text: line })
  })
  return content
}

export function textToDoc(text: string): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph", content: textToInlineContent(text) }],
  }
}
