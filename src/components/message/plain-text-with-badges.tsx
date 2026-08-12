"use client"

import { memo, useMemo } from "react"
import { TriangleAlert } from "lucide-react"
import { useTranslations } from "next-intl"

import { ReferenceBadge } from "@/components/chat/composer/badges/reference-badge"
import { useOpenLinkOrFile } from "@/components/ai-elements/link-safety"
import { cn } from "@/lib/utils"
import type { ReferenceAttrs } from "@/components/chat/composer/types"

import { parseUserMessageSegments } from "./user-message-segments"

function FileReference({
  attrs,
  statusLabel,
}: {
  attrs: ReferenceAttrs
  statusLabel?: string
}) {
  const t = useTranslations("Folder.chat.messageList")
  const openTarget = useOpenLinkOrFile()
  return (
    <button
      type="button"
      title={statusLabel ?? attrs.label}
      aria-label={
        statusLabel ?? t("fileAttachmentLabel", { name: attrs.label })
      }
      onClick={() => void openTarget(attrs.uri!)}
      className="inline-flex max-w-full cursor-pointer appearance-none align-middle leading-none hover:opacity-80"
    >
      <ReferenceBadge data={attrs} />
      {statusLabel && (
        <TriangleAlert
          className="ms-1 h-3.5 w-3.5 shrink-0 self-center text-amber-600"
          aria-hidden
        />
      )}
    </button>
  )
}

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
          segment.attrs.refType === "file" && segment.attrs.uri ? (
            <FileReference key={index} attrs={segment.attrs} />
          ) : (
            <ReferenceBadge key={index} data={segment.attrs} />
          )
        ) : (
          <span key={index}>{segment.text}</span>
        )
      )}
    </div>
  )
})
