"use client"

import { useEffect, useRef, type RefObject } from "react"
import { ChevronUp, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { MessageScrollContextValue } from "./message-scroll-context"

interface HistoryControlsProps {
  historyStart?: number
  canReload: boolean
  loading: boolean
  error: string | null
  onReload: () => void
  onLoadEarlier: () => void
  scrollApiRef: RefObject<MessageScrollContextValue | null>
}

function useScrollToLoadedHistory({
  historyStart,
  error,
  scrollApiRef,
}: HistoryControlsProps) {
  const requestedBefore = useRef<number | null>(null)
  useEffect(() => {
    const before = requestedBefore.current
    if (before === null) return
    if (error) {
      requestedBefore.current = null
      return
    }
    if (historyStart === undefined || historyStart >= before) return
    const frame = requestAnimationFrame(() => {
      scrollApiRef.current?.scrollToIndex(0, { align: "start" })
      requestedBefore.current = null
    })
    return () => cancelAnimationFrame(frame)
  }, [historyStart, error, scrollApiRef])
  return () => {
    requestedBefore.current = historyStart ?? Infinity
  }
}

export function ConversationHistoryControls(props: HistoryControlsProps) {
  const { historyStart, error } = props
  const t = useTranslations("Folder.chat.messageList")
  const requestScroll = useScrollToLoadedHistory(props)
  const hasEarlier = historyStart !== undefined && historyStart > 0
  const canReload = props.canReload && (historyStart === undefined || !!error)
  if (!hasEarlier && !canReload && !error) return null
  const retry = !!error && !hasEarlier
  const Icon = props.loading ? Loader2 : retry ? RefreshCw : ChevronUp
  return (
    <div className="flex shrink-0 flex-col items-center gap-1">
      {(hasEarlier || canReload) && (
        <Button
          size="sm"
          variant="ghost"
          disabled={props.loading}
          aria-busy={props.loading}
          onClick={() => {
            // 仅有实时正文时，先从磁盘读首个历史页，再使用它返回的游标。
            requestScroll()
            if (hasEarlier) props.onLoadEarlier()
            else props.onReload()
          }}
        >
          <Icon aria-hidden className={props.loading ? "animate-spin" : ""} />
          {t(retry ? "errorActionReload" : "loadEarlier")}
        </Button>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive break-words">
          {error}
        </p>
      )}
    </div>
  )
}
