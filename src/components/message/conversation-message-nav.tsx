"use client"

import { memo, useCallback, type RefObject } from "react"
import { ChevronDownIcon, MessageCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import type { MessageScrollContextValue } from "@/components/message/message-scroll-context"
import { CollapsedOverlayChip } from "@/components/chat/collapsed-overlay-chip"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"

/** One navigable user message. */
export interface MessageNavEntry {
  /** Index into the rendered `threadItems` array — fed to `scrollToIndex`. */
  threadIndex: number
  turnId: string
  /** 1-based position among shown entries. */
  ordinal: number
  label: string
}

interface ConversationMessageNavProps {
  /** Number of user messages shown in the collapsed chip. */
  count: number
  /** Whether the panel is expanded. Owned by the parent so it can compute
   *  `entries` lazily — only while open. */
  expanded: boolean
  onToggle: (next: boolean) => void
  /** Per-message rows. Only populated while expanded. */
  entries: MessageNavEntry[]
  scrollApiRef: RefObject<MessageScrollContextValue | null>
}

/**
 * Per-conversation message navigator. Lives in the inline-start overlay stack
 * as the first chip (above the plan and sub-agent panels).
 *
 * Collapsed (default): a bullet-shaped chip showing the message count. Expanded:
 * a compact list that jumps to each user message.
 */
export const ConversationMessageNav = memo(function ConversationMessageNav({
  count,
  expanded,
  onToggle,
  entries,
  scrollApiRef,
}: ConversationMessageNavProps) {
  const t = useTranslations("Folder.chat.messageNav")

  const jump = useCallback(
    (threadIndex: number) => {
      scrollApiRef.current?.scrollToIndex(threadIndex, {
        align: "start",
        smooth: true,
      })
    },
    [scrollApiRef]
  )

  if (count <= 0) return null

  if (!expanded) {
    // Positioning (absolute inline-start/top, column order) is owned by the shared
    // overlay-stack container in MessageListView; the chip only declares its
    // own layout + pointer behavior.
    return (
      <CollapsedOverlayChip
        icon={<MessageCircle className="size-4 sm:size-[18px]" />}
        summary={t("collapsedSummary", { count })}
        onClick={() => onToggle(true)}
      />
    )
  }

  return (
    <div className="pointer-events-none flex max-w-[min(22rem,calc(100%-2rem))]">
      <div className="pointer-events-auto w-72 max-w-full rounded-xl border bg-card/60 hover:bg-card/95 shadow-lg backdrop-blur transition-colors supports-[backdrop-filter]:bg-card/50 supports-[backdrop-filter]:hover:bg-card/85">
        <div className="flex items-center justify-between border-b px-3 py-2">
          <div className="flex min-w-0 items-center gap-2">
            <MessageCircle className="h-4 w-4 text-muted-foreground" />
            <span className="truncate text-sm font-medium">{t("title")}</span>
            <Badge variant="secondary" className="h-5">
              {count}
            </Badge>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={t("collapse")}
            onClick={() => onToggle(false)}
          >
            <ChevronDownIcon className="h-4 w-4" />
          </Button>
        </div>

        <div className="max-h-96 space-y-1.5 overflow-y-auto p-2">
          {entries.map((entry) => (
            <button
              key={entry.turnId}
              type="button"
              onClick={() => jump(entry.threadIndex)}
              className="flex w-full min-w-0 items-start gap-2 rounded-lg border border-border bg-transparent px-2.5 py-2 text-left text-card-foreground transition-colors hover:bg-accent/40"
            >
              <span className="mt-0.5 shrink-0 rounded-md border border-border bg-muted/40 px-1 text-[10px] tabular-nums text-muted-foreground">
                #{entry.ordinal}
              </span>
              <span className="line-clamp-2 min-w-0 flex-1 text-xs leading-5 text-foreground">
                {entry.label}
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
})
