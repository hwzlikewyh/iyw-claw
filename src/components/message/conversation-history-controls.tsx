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
  return (before: number) => {
    requestedBefore.current = before
  }
}

export function ConversationHistoryControls(props: HistoryControlsProps) {
  const { historyStart, error } = props
  const t = useTranslations("Folder.chat.messageList")
  const requestScroll = useScrollToLoadedHistory(props)
  const hasEarlier = historyStart !== undefined && historyStart > 0
  if (!hasEarlier && !error) return null
  const retry = !hasEarlier && props.canReload && !!error
  const Icon = props.loading ? Loader2 : retry ? RefreshCw : ChevronUp
  return (
    <div className="flex shrink-0 flex-col items-center gap-1">
      {(hasEarlier || retry) && (
        <Button
          size="sm"
          variant="ghost"
          disabled={props.loading}
          aria-busy={props.loading}
          onClick={() => {
            if (hasEarlier) {
              requestScroll(historyStart)
              props.onLoadEarlier()
            } else {
              props.onReload()
            }
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
