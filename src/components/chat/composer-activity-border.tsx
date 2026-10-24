"use client"

import { useConversationPresentation } from "./conversation-presentation-context"
import { useRef } from "react"
import { useComposerBorderMotion } from "./composer-border-motion"
import "./composer-activity-border.css"

export function ComposerActivityBorder({ active }: { active: boolean }) {
  const presentation = useConversationPresentation()
  const enabled = active && !presentation?.awaitingInput
  const ref = useRef<HTMLDivElement>(null)
  useComposerBorderMotion(ref, enabled)
  if (!enabled) return null
  return (
    <div ref={ref} className="conversation-composer-runner" aria-hidden="true">
      <span className="conversation-composer-marker" />
    </div>
  )
}
