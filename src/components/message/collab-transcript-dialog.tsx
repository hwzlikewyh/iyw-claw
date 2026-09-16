"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
import { adaptMessageTurns } from "@/lib/adapters/ai-elements-adapter"
import type { ConversationDetail } from "@/lib/types"
import { MessageBubble } from "./message-bubble"

interface Props {
  open: boolean
  onOpenChange: (open: boolean) => void
  detail: ConversationDetail | null
  name: string
}

function useTranscriptMessages(detail: ConversationDetail | null) {
  const shared = useTranslations("Folder.chat.shared")
  const errors = useTranslations("Folder.chat.acpConnections.backendErrors")
  return useMemo(
    () =>
      adaptMessageTurns(detail?.turns ?? [], {
        attachedResources: shared("attachedResources"),
        toolCallFailed: shared("toolCallFailed"),
        runtimeErrors: {
          insufficientBalance: errors("insufficientBalance"),
          authenticationFailed: errors("authenticationFailed"),
          permissionDenied: errors("permissionDenied"),
          rateLimited: errors("rateLimited"),
          quotaExceeded: errors("quotaExceeded"),
          modelUnavailable: errors("modelUnavailable"),
          requestTimeout: errors("requestTimeout"),
          networkError: errors("networkError"),
          serviceUnavailable: errors("serviceUnavailable"),
          requestFailed: errors("requestFailed"),
        },
      }),
    [detail, shared, errors]
  )
}

export function CollabTranscriptDialog({
  open,
  onOpenChange,
  detail,
  name,
}: Props) {
  const t = useTranslations("Folder.chat.collabAgent")
  const messages = useTranscriptMessages(detail)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[85dvh] w-[calc(100%-2rem)] max-w-3xl flex-col gap-0 overflow-hidden rounded-lg p-0">
        <div className="border-b px-4 py-3 pr-12">
          <DialogTitle className="break-all text-sm">{name}</DialogTitle>
          <DialogDescription>{t("openTranscript")}</DialogDescription>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {messages.length ? (
            messages.map((message) => (
              <MessageBubble key={message.id} message={message} />
            ))
          ) : (
            <p className="p-4 text-sm text-muted-foreground">
              {t("transcriptPending")}
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
