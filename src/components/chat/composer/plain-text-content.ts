import type { JSONContent } from "@tiptap/core"
import {
  tokenizeReferenceLinks,
  unescapeReferenceLabel,
} from "@/lib/reference-link"
import { parseElementReferenceUri } from "./element-reference"

function appendPlainText(text: string, content: JSONContent[]): void {
  text.split("\n").forEach((line, index) => {
    if (index > 0) content.push({ type: "hardBreak" })
    if (line.length > 0) content.push({ type: "text", text: line })
  })
}

export function textToInlineContent(text: string): JSONContent[] {
  if (!text) return []
  const content: JSONContent[] = []
  for (const token of tokenizeReferenceLinks(text)) {
    if (token.type === "link") {
      const destination = token.destination.replace(/^<|>$/g, "")
      const attrs = parseElementReferenceUri(
        destination,
        unescapeReferenceLabel(token.label)
      )
      if (attrs) content.push({ type: "reference", attrs })
      else appendPlainText(token.raw, content)
    } else {
      appendPlainText(token.value, content)
    }
  }
  return content
}

export function textToDoc(text: string): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph", content: textToInlineContent(text) }],
  }
}
