"use client"

import { useId, type ReactNode } from "react"
import { AssistantArchiveToggle } from "./assistant-archive-toggle"
import { AssistantFoldContent } from "./assistant-fold-content"
import { useTranslations } from "next-intl"
import { useArchiveDisclosure } from "./use-archive-disclosure"

export function AssistantContentSegment({
  archive,
  children,
}: {
  archive: boolean
  children: ReactNode
}) {
  const t = useTranslations("Folder.chat.messageList")
  const contentId = useId()
  const { rootRef, open, onOpenChange } = useArchiveDisclosure(archive)
  const visible = !archive || open
  return (
    <div ref={rootRef} className="assistant-content-segment">
      {archive && (
        <AssistantArchiveToggle
          open={visible}
          contentId={contentId}
          label={t(visible ? "collapseResponse" : "expandResponse")}
          onToggle={() => onOpenChange(!visible)}
        />
      )}
      <AssistantFoldContent open={visible} contentId={contentId}>
        {children}
      </AssistantFoldContent>
    </div>
  )
}
