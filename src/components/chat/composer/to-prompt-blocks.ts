import type { Editor } from "@tiptap/core"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"

import type { PromptInputBlock } from "@/lib/types"

import { referenceToMarkdown } from "./reference-text"
import { isEmbeddedReferenceUri } from "./reference-uri"
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
 * The whole document serializes to a single text block (no mid-paragraph
 * fragmentation), with every reference sitting inline exactly where the sender
 * placed it.
 */
export function docToPromptBlocks(editor: Editor): PromptInputBlock[] {
  const text = serializeDocToText(editor.state.doc).trim()
  return text ? [{ type: "text", text }] : []
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
