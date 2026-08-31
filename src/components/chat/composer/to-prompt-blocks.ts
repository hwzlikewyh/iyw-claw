import type { Editor } from "@tiptap/core"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"

import type { PromptInputBlock } from "@/lib/types"

import { referenceToMarkdown } from "./reference-text"
import { isEmbeddedReferenceUri } from "./reference-uri"
import { scenarioVariableText } from "./nodes/scenario-variable-node"
import type { ImageAttachmentAttrs } from "./nodes/image-attachment-node"
import type { ReferenceAttrs } from "./types"

/**
 * Send serialization: turn the composer document into the prose portion of a
 * `PromptInputBlock[]`. (Out-of-band image / embedded-byte attachments are
 * appended by the host's `buildDraft`; this function owns only the editor doc.)
 *
 * Every reference EXCEPT an embedded-attachment ref serializes **inline, in
 * place**, via the node's own `renderMarkdown` (see
 * {@link "./reference-text".referenceToMarkdown}):
 *
 * - **file** references render as an inline `[label](file://uri)` Markdown link
 *   at the exact position they were typed. They are deliberately *not* lifted
 *   into trailing `resource_link` blocks: iyw-claw keeps no copy of the user's
 *   prompt, so on cold reload the message is reparsed from the agent's own
 *   session file — and only what stays inline in the text survives at its
 *   original position. A trailing ResourceLink ends up stored/reparsed at the
 *   *end* of the message (or dropped entirely — e.g. Claude's parser ignores the
 *   resulting `document` block), which is why a file badge used to jump to the
 *   end of the bubble after reopening a conversation. Keeping the link inline
 *   fixes that for every agent. For a local `file://` an ACP ResourceLink only
 *   conveys the path anyway — identical information to the inline link — so
 *   nothing is lost on the agent side.
 * - **session / commit** references (a `iyw-claw://` uri the agent can't fetch) and
 *   **agent / skill** references stay inline as their text/link form, unchanged.
 * - **embedded** references (a `iyw-claw://embedded/…` display uri for path-less
 *   pasted bytes) are dropped from the prose: their real bytes-bearing block is
 *   appended separately by the host's `buildDraft` (keyed on the same uri via the
 *   send-time payload map), so emitting their synthetic display link here would
 *   leak a uri the agent shouldn't see.
 *
 * Text is merged only across adjacent text/reference content. Inline image
 * attachment nodes intentionally split the block stream so their position is
 * preserved exactly where the sender placed them.
 */
export interface DocToPromptBlocksOptions {
  resolveImage?: (
    attrs: ImageAttachmentAttrs
  ) => Extract<PromptInputBlock, { type: "image" }> | null
}

function appendTextBlock(blocks: PromptInputBlock[], text: string): void {
  if (!text) return
  const previous = blocks[blocks.length - 1]
  if (previous?.type === "text") {
    previous.text += text
  } else {
    blocks.push({ type: "text", text })
  }
}

function appendDocumentNode(
  node: ProseMirrorNode,
  blocks: PromptInputBlock[],
  options: DocToPromptBlocksOptions
): void {
  if (node.type.name === "imageAttachment") {
    const image = options.resolveImage?.(node.attrs as ImageAttachmentAttrs)
    if (image) {
      blocks.push(image)
    } else {
      appendTextBlock(
        blocks,
        `[${String((node.attrs as ImageAttachmentAttrs).name || "image")}]`
      )
    }
    return
  }
  if (node.isText) {
    appendTextBlock(blocks, node.text ?? "")
    return
  }
  if (node.type.name === "hardBreak") {
    appendTextBlock(blocks, "\n")
    return
  }
  if (node.isLeaf) {
    appendTextBlock(blocks, composerLeafText(node))
    return
  }
  node.forEach((child) => appendDocumentNode(child, blocks, options))
}

export function docToPromptBlocks(
  editor: Editor,
  options: DocToPromptBlocksOptions = {}
): PromptInputBlock[] {
  const blocks: PromptInputBlock[] = []
  editor.state.doc.forEach((child, index) => {
    if (index > 0) appendTextBlock(blocks, "\n")
    appendDocumentNode(child, blocks, options)
  })
  const first = blocks[0]
  if (first?.type === "text") first.text = first.text.trimStart()
  const last = blocks[blocks.length - 1]
  if (last?.type === "text") last.text = last.text.trimEnd()
  return blocks.filter(
    (block) => block.type !== "text" || block.text.length > 0
  )
}

export function composerLeafText(
  leaf: ProseMirrorNode,
  options?: { keepEmbedded?: boolean }
): string {
  if (leaf.type.name === "reference") {
    const attrs = leaf.attrs as ReferenceAttrs
    if (
      !options?.keepEmbedded &&
      typeof attrs.uri === "string" &&
      isEmbeddedReferenceUri(attrs.uri)
    ) {
      return ""
    }
    return referenceToMarkdown(attrs)
  }
  if (leaf.type.name === "scenarioVariable") {
    return scenarioVariableText(leaf.attrs)
  }
  if (leaf.type.name === "hardBreak") return "\n"
  return ""
}

export function serializeDocToText(doc: ProseMirrorNode): string {
  return doc.textBetween(0, doc.content.size, "\n", composerLeafText)
}

export function serializeDocToDisplayText(doc: ProseMirrorNode): string {
  return doc.textBetween(0, doc.content.size, "\n", (leaf) =>
    composerLeafText(leaf, { keepEmbedded: true })
  )
}
