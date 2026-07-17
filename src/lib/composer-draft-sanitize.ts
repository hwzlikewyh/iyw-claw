import type { JSONContent } from "@tiptap/core"

const KEPT_INLINE = new Set(["text", "hardBreak", "reference"])
const INLINE_CONTENT_BLOCK = new Set(["paragraph", "heading", "codeBlock"])
const BLOCK_CONTENT_BLOCK = new Set([
  "blockquote",
  "bulletList",
  "orderedList",
  "listItem",
  "taskList",
  "taskItem",
])

function isBlockType(type: string | undefined): boolean {
  return (
    !!type && (INLINE_CONTENT_BLOCK.has(type) || BLOCK_CONTENT_BLOCK.has(type))
  )
}

function sanitizeInline(nodes: JSONContent[] | undefined): JSONContent[] {
  const output: JSONContent[] = []
  for (const node of nodes ?? []) {
    const type = node?.type
    if (type === "text") {
      if (typeof node.text === "string" && node.text.length > 0) {
        output.push({ type: "text", text: node.text })
      }
    } else if (type === "hardBreak") {
      output.push({ type: "hardBreak" })
    } else if (type === "reference") {
      output.push({ type: "reference", attrs: node.attrs })
    } else if (Array.isArray(node?.content)) {
      output.push(...sanitizeInline(node.content))
    }
  }
  return output
}

function sanitizeBlocks(
  nodes: JSONContent[] | undefined,
  output: JSONContent[]
): void {
  for (const node of nodes ?? []) {
    const type = node?.type
    if (type && INLINE_CONTENT_BLOCK.has(type)) {
      output.push({ type: "paragraph", content: sanitizeInline(node.content) })
    } else if (type && BLOCK_CONTENT_BLOCK.has(type)) {
      sanitizeBlocks(node.content, output)
    } else if (Array.isArray(node?.content)) {
      if (node.content.some((child) => isBlockType(child?.type))) {
        sanitizeBlocks(node.content, output)
      } else {
        output.push({
          type: "paragraph",
          content: sanitizeInline(node.content),
        })
      }
    }
  }
}

function isPlainSchemaDoc(doc: JSONContent): boolean {
  if (!Array.isArray(doc.content) || doc.content.length === 0) return false
  for (const block of doc.content) {
    if (block?.type !== "paragraph") return false
    for (const inline of block.content ?? []) {
      if (!KEPT_INLINE.has(inline?.type ?? "")) return false
      if (Array.isArray(inline.marks) && inline.marks.length > 0) return false
    }
  }
  return true
}

export function sanitizeComposerDraftDoc(doc: JSONContent): JSONContent {
  if (isPlainSchemaDoc(doc)) return doc
  const content: JSONContent[] = []
  sanitizeBlocks(doc.content, content)
  return {
    type: "doc",
    content: content.length > 0 ? content : [{ type: "paragraph" }],
  }
}
