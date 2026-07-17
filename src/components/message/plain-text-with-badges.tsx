"use client"

import { memo, useMemo } from "react"

import { ReferenceBadge } from "@/components/chat/composer/badges/reference-badge"
import { cn } from "@/lib/utils"

import { parseUserMessageSegments } from "./user-message-segments"

export const PlainTextWithBadges = memo(function PlainTextWithBadges({
  text,
  className,
}: {
  text: string
  className?: string
}) {
  const segments = useMemo(() => parseUserMessageSegments(text), [text])
  return (
    <div className={cn("whitespace-pre-wrap break-words", className)}>
      {segments.map((segment, index) =>
        segment.kind === "reference" ? (
          <ReferenceBadge key={index} data={segment.attrs} />
        ) : (
          <span key={index}>{segment.text}</span>
        )
      )}
    </div>
  )
})
