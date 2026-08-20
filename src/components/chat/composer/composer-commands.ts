import type { Editor } from "@tiptap/core"

import type { PromptInputBlock } from "@/lib/types"

import type { InputAttachment } from "../message-input-attachments"
import { blocksToRestoredDraft } from "./from-prompt-blocks"
import { textToInlineContent } from "./plain-text-content"
import type { ReferenceAttrs } from "./types"

/**
 * Whether the composer has nothing sendable. Stricter than `editor.isEmpty`,
 * which is false for a whitespace-only document (the legacy textarea gated the
 * send button on `text.trim()`), but still treats a document holding only an
 * inline reference badge (e.g. an `@file` mention with no prose) as sendable.
 */
export function isComposerEmpty(editor: Editor): boolean {
  if (editor.isEmpty) return true
  if (editor.getText().trim().length > 0) return false
  let hasReference = false
  editor.state.doc.descendants((node) => {
    if (hasReference) return false
    if (node.type.name === "reference") {
      hasReference = true
      return false
    }
    return true
  })
  return !hasReference
}

// Elements that own their own click behavior: the editor surface, interactive
// controls, and inline badges. A mousedown landing on any of these (or a
// descendant) is NOT an "empty chrome" click.
const NON_CHROME_SELECTOR =
  '.ProseMirror, button, a, input, textarea, select, [role="button"], [role="combobox"], [role="menuitem"], [data-reference-badge], [contenteditable]'

function normalizedReferenceId(attrs: ReferenceAttrs): string {
  return attrs.id
    .trim()
    .replace(/^[/$]+/, "")
    .toLowerCase()
}

export function isTaskReference(attrs: ReferenceAttrs): boolean {
  const id = normalizedReferenceId(attrs)
  return attrs.refType === "skill" && (id === "goal" || id === "loop")
}

function isExpertReference(attrs: ReferenceAttrs): boolean {
  return attrs.refType === "skill" && attrs.meta?.scope === "expert"
}

function latestDirectiveReferences(editor: Editor): {
  task: ReferenceAttrs | null
  expert: ReferenceAttrs | null
} {
  let task: ReferenceAttrs | null = null
  let expert: ReferenceAttrs | null = null
  editor.state.doc.descendants((node) => {
    if (node.type.name !== "reference") return true
    const attrs = node.attrs as ReferenceAttrs
    if (isTaskReference(attrs)) task = attrs
    if (isExpertReference(attrs)) expert = attrs
    return true
  })
  return { task, expert }
}

export function getTaskReference(editor: Editor): ReferenceAttrs | null {
  return latestDirectiveReferences(editor).task
}

function directiveRanges(editor: Editor) {
  const ranges: { from: number; to: number }[] = []
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name !== "reference") return true
    const attrs = node.attrs as ReferenceAttrs
    if (!isTaskReference(attrs) && !isExpertReference(attrs)) return true
    const after = editor.state.doc.resolve(pos + node.nodeSize).nodeAfter
    const trailingSpace = after?.isText && after.text?.startsWith(" ") ? 1 : 0
    ranges.push({ from: pos, to: pos + node.nodeSize + trailingSpace })
    return true
  })
  return ranges
}

function applyDirectiveReferences(
  editor: Editor,
  task: ReferenceAttrs | null,
  expert: ReferenceAttrs | null,
  focus = true
): void {
  const content = [task, expert].flatMap((attrs) =>
    attrs
      ? [
          { type: "reference", attrs },
          { type: "text", text: " " },
        ]
      : []
  )
  let chain = editor.chain()
  if (focus) chain = chain.focus()
  for (const range of directiveRanges(editor).reverse()) {
    chain = chain.deleteRange(range)
  }
  if (content.length === 0) {
    if (focus) chain = chain.setTextSelection(1)
    chain.run()
    return
  }
  const first = editor.state.doc.firstChild
  if (!first || first.type.name !== "paragraph") {
    chain.insertContentAt(0, { type: "paragraph", content }).run()
    return
  }
  chain = chain.insertContentAt(1, content)
  if (focus) chain = chain.setTextSelection(1 + content.length)
  chain.run()
}

export function applyTaskReference(
  editor: Editor,
  attrs: ReferenceAttrs
): void {
  const { expert } = latestDirectiveReferences(editor)
  applyDirectiveReferences(editor, attrs, expert)
}

export function clearTaskReference(editor: Editor): void {
  const { expert } = latestDirectiveReferences(editor)
  applyDirectiveReferences(editor, null, expert)
}

export function normalizeDirectiveReferences(editor: Editor): void {
  const { task, expert } = latestDirectiveReferences(editor)
  if (task || expert) applyDirectiveReferences(editor, task, expert, false)
}

/**
 * Whether a mousedown `target` landed on the message input's empty chrome — its
 * padding, the blank space below a short message, or the gaps in the action bar
 * — rather than on the editor surface or an interactive control. The host uses
 * this to focus the editor when the user clicks the otherwise-dead space around
 * it (only the editor surface itself used to be clickable).
 */
export function isComposerChromeClick(target: EventTarget | null): boolean {
  return target instanceof Element && !target.closest(NON_CHROME_SELECTOR)
}

/**
 * Insert an expert as a leading whole-turn directive. A selected task stays
 * first; the expert is normalized immediately after it. Without a task the
 * expert remains the first inline badge.
 *
 * Task and expert references are each unique. Existing copies are removed from
 * the document before the normalized directive pair is inserted.
 */
export function applyExpertReference(
  editor: Editor,
  attrs: ReferenceAttrs
): void {
  const { task } = latestDirectiveReferences(editor)
  applyDirectiveReferences(editor, task, attrs)
}

export function restampSkillPrefixes(
  editor: Editor,
  prefix: "/" | "$"
): boolean {
  const updates: { pos: number; attrs: ReferenceAttrs }[] = []
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name !== "reference") return true
    const attrs = node.attrs as ReferenceAttrs
    if (
      attrs.refType === "skill" &&
      attrs.meta?.scope != null &&
      attrs.meta.invocationPrefix !== prefix
    ) {
      updates.push({
        pos,
        attrs: {
          ...attrs,
          meta: { ...attrs.meta, invocationPrefix: prefix },
        },
      })
    }
    return true
  })
  if (updates.length === 0) return false

  const transaction = editor.state.tr
  for (const { pos, attrs } of updates) {
    transaction.setNodeMarkup(pos, undefined, attrs)
  }
  transaction.setMeta("addToHistory", false)
  editor.view.dispatch(transaction)
  return true
}

/**
 * Replay a previously-sent `PromptInputBlock[]` (a queued message's draft) back
 * into the editor: prose + reference badges in order, returning the out-of-band
 * attachments (images / embedded resources / non-composer links) for the host to
 * set. Inverse of `docToPromptBlocks` for the queue-edit round-trip. The editor
 * is cleared first so this fully replaces the current content.
 */
export function restoreBlocksIntoEditor(
  editor: Editor,
  blocks: PromptInputBlock[]
): InputAttachment[] {
  const { segments, attachments } = blocksToRestoredDraft(blocks)
  let chain = editor.chain().clearContent()
  for (const segment of segments) {
    chain =
      segment.kind === "text"
        ? chain.insertContent(textToInlineContent(segment.text))
        : chain.insertReference(segment.attrs)
  }
  chain.focus("end").run()
  normalizeDirectiveReferences(editor)
  return attachments
}
