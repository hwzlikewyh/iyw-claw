import { useEffect, useRef, useState, type ReactNode } from "react"
import { useTranslations } from "next-intl"
import { ArrowDown } from "lucide-react"
import { useProcessAutoFollow } from "./use-process-auto-follow"
import { LIVE_TEXT_MAX_HEIGHT } from "./process-presentation"

interface StreamingTextSurfaceProps {
  text: string
  compact: boolean
  reading: boolean
  children: ReactNode
  onExpandedChange?: (expanded: boolean) => void
}

function StreamingTextControls({
  expanded,
  following,
  onToggle,
  onLatest,
}: {
  expanded: boolean
  following: boolean
  onToggle: () => void
  onLatest: () => void
}) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="mt-2 flex items-center gap-3 text-xs text-muted-foreground">
      <button
        type="button"
        className="assistant-text-action"
        aria-expanded={expanded}
        onClick={onToggle}
      >
        {t(expanded ? "collapseResponse" : "expandResponse")}
      </button>
      {!expanded && !following && (
        <button
          type="button"
          className="assistant-text-action inline-flex items-center gap-1"
          onClick={onLatest}
        >
          <ArrowDown className="size-3" />
          {t("backToLatest")}
        </button>
      )}
    </div>
  )
}

// 保持同一份 Markdown DOM；长正文在独立视口内收纳，避免拆断代码和表格。
export function StreamingTextSurface({
  text,
  compact,
  reading,
  children,
  onExpandedChange,
}: StreamingTextSurfaceProps) {
  const [expanded, setExpanded] = useState(false)
  const [overflowing, setOverflowing] = useState(false)
  const contentRef = useRef<HTMLDivElement>(null)
  const folded = compact && !expanded
  const { viewportRef, handleScroll, isFollowing, scrollToLatest } =
    useProcessAutoFollow(text, folded && !reading)
  useEffect(() => {
    const content = contentRef.current
    if (!content) return
    let frame: number | null = null
    const observer = new ResizeObserver(() => {
      if (frame !== null) return
      frame = requestAnimationFrame(() => {
        frame = null
        const next = content.scrollHeight > LIVE_TEXT_MAX_HEIGHT
        setOverflowing((previous) => (previous === next ? previous : next))
      })
    })
    observer.observe(content)
    return () => {
      if (frame !== null) cancelAnimationFrame(frame)
      observer.disconnect()
    }
  }, [])
  return (
    <div className="relative">
      <div
        ref={viewportRef}
        onScroll={handleScroll}
        className={
          folded
            ? "assistant-live-text-viewport overflow-y-auto overscroll-contain"
            : undefined
        }
        style={folded ? { maxHeight: LIVE_TEXT_MAX_HEIGHT } : undefined}
      >
        <div ref={contentRef}>{children}</div>
      </div>
      {compact && overflowing && (
        <StreamingTextControls
          expanded={expanded}
          following={isFollowing}
          onToggle={() => {
            setExpanded(!expanded)
            onExpandedChange?.(!expanded)
          }}
          onLatest={scrollToLatest}
        />
      )}
    </div>
  )
}
