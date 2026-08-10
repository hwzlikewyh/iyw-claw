"use client"

import { CircleAlert, LoaderCircle, RotateCcw, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { PlainTextWithBadges } from "@/components/message/plain-text-with-badges"
import { UserImageAttachments } from "@/components/message/user-image-attachments"
import { UserResourceLinks } from "@/components/message/user-resource-links"
import {
  extractUserImagesFromDraft,
  extractUserResourcesFromDraft,
} from "@/lib/prompt-draft"
import type { AgentInputItem, PromptDraft } from "@/lib/types"

interface AgentInputWaitingDisplayProps {
  items: AgentInputItem[]
  onDelete?: (id: string) => void
  onRetry?: (id: string) => void
}

function toDraft(item: AgentInputItem): PromptDraft {
  return {
    blocks: item.payload.blocks,
    displayText: item.payload.display_text,
  }
}

export function AgentInputWaitingDisplay({
  items,
  onDelete,
  onRetry,
}: AgentInputWaitingDisplayProps) {
  const t = useTranslations("Folder.chat.agentInput")
  const visible = items.filter((item) =>
    ["waiting", "dispatching", "fallback_queued", "failed"].includes(
      item.status
    )
  )
  if (visible.length === 0) return null

  return (
    <div className="max-h-40 space-y-1 overflow-y-auto pb-1">
      {visible.map((item) => {
        const draft = toDraft(item)
        const images = extractUserImagesFromDraft(draft)
        const resources = extractUserResourcesFromDraft(draft)
        const failed = item.status === "failed"
        return (
          <div
            key={item.id}
            className="flex min-w-0 items-start gap-2 rounded-md border border-border/70 bg-muted/35 px-2 py-1.5"
          >
            {failed ? (
              <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-destructive" />
            ) : (
              <LoaderCircle className="mt-0.5 size-3.5 shrink-0 animate-spin text-muted-foreground" />
            )}
            <div className="min-w-0 flex-1 space-y-1">
              <div className="text-[10px] font-medium text-muted-foreground">
                {failed ? t("failed") : t("waiting")}
              </div>
              {images.length > 0 && <UserImageAttachments images={images} />}
              {item.payload.display_text.trim() && (
                <PlainTextWithBadges
                  text={item.payload.display_text}
                  className="line-clamp-3 text-xs text-foreground/85"
                />
              )}
              {!item.payload.display_text.trim() && resources.length > 0 && (
                <UserResourceLinks resources={resources} />
              )}
            </div>
            {failed && onRetry && (
              <button
                type="button"
                onClick={() => onRetry(item.id)}
                className="shrink-0 rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                title={t("retry")}
                aria-label={t("retry")}
              >
                <RotateCcw className="size-3" />
              </button>
            )}
            {item.status === "waiting" && onDelete && (
              <button
                type="button"
                onClick={() => onDelete(item.id)}
                className="shrink-0 rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                title={t("delete")}
                aria-label={t("delete")}
              >
                <X className="size-3" />
              </button>
            )}
          </div>
        )
      })}
    </div>
  )
}
