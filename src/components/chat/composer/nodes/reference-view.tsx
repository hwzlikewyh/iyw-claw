import { NodeViewWrapper, type ReactNodeViewProps } from "@tiptap/react"

import { ReferenceBadge } from "../badges/reference-badge"
import { isTaskReference } from "../composer-commands"
import type { ReferenceAttrs } from "../types"

/**
 * React node view for the `reference` atom. Renders the inline badge and marks
 * the surface non-editable so the caret treats the whole reference as one unit.
 */
export function ReferenceView({ node }: ReactNodeViewProps) {
  const attrs = node.attrs as ReferenceAttrs
  const task = isTaskReference(attrs)
  return (
    <NodeViewWrapper
      as="span"
      className={task ? "iyw-claw-task-reference" : "iyw-claw-reference"}
      contentEditable={false}
      aria-hidden={task || undefined}
    >
      <ReferenceBadge data={attrs} />
    </NodeViewWrapper>
  )
}
