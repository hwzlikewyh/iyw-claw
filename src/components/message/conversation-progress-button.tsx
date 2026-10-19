"use client"

import { useEffect, useRef } from "react"
import { ArrowDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { useStickToBottomContext } from "use-stick-to-bottom"
import { useConversationActivityVisibility } from "./conversation-activity-visibility"

interface ProgressButtonProps {
  version: unknown
  latestId?: string
  prompting: boolean
  onReturnToReply?: () => void
}

export function ConversationProgressButton(props: ProgressButtonProps) {
  const { isAtBottom, scrollToBottom } = useStickToBottomContext()
  const { unreadProgress, setUnreadProgress, outputVisible } =
    useConversationActivityVisibility()
  const previous = useRef({ version: props.version, id: props.latestId })
  const t = useTranslations("ConversationPresentation")
  useEffect(() => {
    const changed =
      previous.current.version !== props.version ||
      previous.current.id !== props.latestId
    previous.current = { version: props.version, id: props.latestId }
    if (isAtBottom || outputVisible) setUnreadProgress(false)
    else if (changed) setUnreadProgress(true)
  }, [
    props.version,
    props.latestId,
    isAtBottom,
    outputVisible,
    setUnreadProgress,
  ])
  if (isAtBottom || outputVisible || props.prompting) return null
  return (
    <button
      type="button"
      onClick={() => {
        setUnreadProgress(false)
        if (props.onReturnToReply) props.onReturnToReply()
        else void scrollToBottom()
      }}
      className="conversation-progress-button absolute bottom-3 left-1/2 flex max-w-[calc(100%-2rem)] -translate-x-1/2 items-center gap-2 rounded-full border bg-background/95 px-3 py-2 text-xs shadow-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <ArrowDown aria-hidden="true" className="size-3.5 shrink-0" />
      <span>{t(unreadProgress ? "newProgress" : "returnLatest")}</span>
    </button>
  )
}
