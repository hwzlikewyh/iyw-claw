"use client"

import { useConversationPresentation } from "./conversation-presentation-context"
import { useDocumentVisibility } from "@/hooks/use-document-visibility"
import "./composer-activity-border.css"

export function ComposerActivityBorder({ active }: { active: boolean }) {
  const visible = useDocumentVisibility()
  const presentation = useConversationPresentation()
  if (!active || presentation?.awaitingInput) return null
  return (
    <svg
      className="conversation-composer-runner"
      aria-hidden="true"
      data-paused={!visible}
    >
      <rect x="1" y="1" rx="12" pathLength="100" />
    </svg>
  )
}
