"use client"

import { useState, type ReactNode } from "react"
import type { AdaptedContentPart } from "@/lib/adapters/ai-elements-adapter"
import { AssistantResponseSurface } from "./assistant-response-surface"
import { StreamingTextSurface } from "./streaming-text-surface"
import { useSettledTurnCompletion } from "./use-settled-turn-completion"
import { useContentReading } from "./use-content-reading"

export function AssistantTextSurface({
  parts,
  streaming,
  compact,
  animationEnabled,
  children,
}: {
  parts: Extract<AdaptedContentPart, { type: "text" }>[]
  streaming: boolean
  compact: boolean
  animationEnabled: boolean
  children: ReactNode
}) {
  const { contentRef, complete, wasStreaming } =
    useSettledTurnCompletion(!streaming)
  const reading = useContentReading(contentRef)
  const [expanded, setExpanded] = useState(false)
  return (
    <div ref={contentRef}>
      {complete ? (
        <AssistantResponseSurface
          parts={parts}
          collapse={compact}
          animateCompletion={animationEnabled && wasStreaming}
          initiallyExpanded={expanded}
        >
          {children}
        </AssistantResponseSurface>
      ) : (
        <StreamingTextSurface
          text={parts.map((part) => part.text).join("\n\n")}
          compact={compact}
          reading={reading}
          onExpandedChange={setExpanded}
        >
          {children}
        </StreamingTextSurface>
      )}
    </div>
  )
}
