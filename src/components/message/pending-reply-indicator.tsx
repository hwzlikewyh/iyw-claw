"use client"

import { Clock3 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Message, MessageContent } from "@/components/ai-elements/message"
import { useConversationActivityAnchor } from "./conversation-activity-visibility"

export function PendingReplyIndicator() {
  const t = useTranslations("Folder.chat.liveTurnStats")
  const ref = useConversationActivityAnchor("activity")
  return (
    <Message from="assistant">
      <MessageContent>
        <div
          ref={ref}
          className="conversation-current-activity py-1"
          role="status"
        >
          <Clock3 aria-hidden="true" className="size-4 shrink-0" />
          <strong>{t("activityUnknown")}</strong>
        </div>
      </MessageContent>
    </Message>
  )
}
