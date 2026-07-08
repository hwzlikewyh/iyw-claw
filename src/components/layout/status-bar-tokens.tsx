"use client"

import { useCallback, useSyncExternalStore } from "react"
import { Coins } from "lucide-react"
import { useTranslations } from "next-intl"
import { useSessionStats } from "@/contexts/session-stats-context"
import { useOptionalConnectionStore } from "@/contexts/acp-connections-context"
import { formatTokenCount } from "@/lib/token-format"
import { formatContextWindowPercent } from "@/lib/context-window"
import type { SessionStats } from "@/lib/types"
import { cn } from "@/lib/utils"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

const ICON_RADIUS = 6
const ICON_CENTER = 8
const ICON_VIEWBOX = 16
const ICON_CIRCUMFERENCE = 2 * Math.PI * ICON_RADIUS

type TokenRowKey = "input" | "output" | "cacheRead" | "cacheWrite" | "total"

interface SessionUsageData {
  contextUsed: number | null
  contextMax: number | null
  contextPercent: number | null
  dashOffset: number
  rows: { key: TokenRowKey; value: number }[]
  total: number | null
  hasContext: boolean
  hasUsage: boolean
  hasTokenSection: boolean
}

interface SessionUsageSourceProps {
  contextKey?: string | null
  sessionStats?: SessionStats | null
}

interface SessionUsageButtonProps extends SessionUsageSourceProps {
  variant?: "status" | "chip"
  className?: string
  popoverSide?: "top" | "right" | "bottom" | "left"
  stopPropagation?: boolean
  showIcon?: boolean
}

function useSessionUsageData({
  contextKey,
  sessionStats: sessionStatsOverride,
}: SessionUsageSourceProps): SessionUsageData | null {
  const store = useOptionalConnectionStore()
  const { sessionStats: contextSessionStats } = useSessionStats()
  const sessionStats =
    sessionStatsOverride !== undefined
      ? sessionStatsOverride
      : contextSessionStats
  const usage = sessionStats?.total_usage

  const shouldUseActiveKey = contextKey === undefined
  const subscribeActiveKey = useCallback(
    (cb: () => void) =>
      shouldUseActiveKey && store ? store.subscribeActiveKey(cb) : () => {},
    [store, shouldUseActiveKey]
  )
  const getActiveKey = useCallback(
    () => (shouldUseActiveKey && store ? store.getActiveKey() : null),
    [store, shouldUseActiveKey]
  )
  const activeKey = useSyncExternalStore(
    subscribeActiveKey,
    getActiveKey,
    getActiveKey
  )
  const targetKey = contextKey === undefined ? activeKey : contextKey

  const subscribeConn = useCallback(
    (cb: () => void) => {
      if (!targetKey || !store) return () => {}
      return store.subscribeKey(targetKey, cb)
    },
    [store, targetKey]
  )
  const getConnSnapshot = useCallback(
    () => (targetKey && store ? store.getConnection(targetKey) : undefined),
    [store, targetKey]
  )
  const activeConn = useSyncExternalStore(
    subscribeConn,
    getConnSnapshot,
    getConnSnapshot
  )

  const rawLiveUsed = activeConn?.usage?.used ?? null
  const rawLiveSize = activeConn?.usage?.size ?? null
  // Treat live used=0 as "no data" so we fall back to sessionStats —
  // Claude Code sends used=0 for synthetic local commands (/context etc.)
  const liveContextUsed =
    rawLiveUsed != null && rawLiveUsed > 0 ? rawLiveUsed : null
  const liveContextMax =
    rawLiveSize != null && rawLiveSize > 0 ? rawLiveSize : null

  const contextUsed =
    liveContextUsed ?? sessionStats?.context_window_used_tokens ?? null
  const contextMax =
    liveContextMax ?? sessionStats?.context_window_max_tokens ?? null
  const contextPercentRaw =
    (liveContextUsed != null && liveContextMax != null && liveContextMax > 0
      ? (liveContextUsed / liveContextMax) * 100
      : sessionStats?.context_window_usage_percent) ??
    (contextUsed != null && contextMax != null && contextMax > 0
      ? (contextUsed / contextMax) * 100
      : null)
  const contextPercent =
    contextPercentRaw == null
      ? null
      : Math.max(0, Math.min(100, contextPercentRaw))
  const hasContext = contextPercent != null
  const hasUsage = usage != null
  const fallbackTotal = hasUsage
    ? usage.input_tokens +
      usage.output_tokens +
      usage.cache_creation_input_tokens +
      usage.cache_read_input_tokens
    : null
  const total = sessionStats?.total_tokens ?? fallbackTotal

  const dashOffset = ICON_CIRCUMFERENCE * (1 - (contextPercent ?? 0) / 100)

  const rows: { key: TokenRowKey; value: number }[] = []
  if (hasUsage) {
    rows.push(
      { key: "input", value: usage.input_tokens },
      { key: "output", value: usage.output_tokens },
      { key: "cacheRead", value: usage.cache_read_input_tokens },
      { key: "cacheWrite", value: usage.cache_creation_input_tokens }
    )
  }
  if (total != null) {
    rows.push({ key: "total", value: total })
  }

  const hasTokenSection = rows.length > 0

  if (!hasContext && !hasTokenSection) return null

  return {
    contextUsed,
    contextMax,
    contextPercent,
    dashOffset,
    rows,
    total,
    hasContext,
    hasUsage,
    hasTokenSection,
  }
}

function SessionUsageButton({
  contextKey,
  sessionStats,
  variant = "status",
  className,
  popoverSide = "top",
  stopPropagation = false,
  showIcon = true,
}: SessionUsageButtonProps) {
  const t = useTranslations("Folder.statusBar.tokens")
  const data = useSessionUsageData({ contextKey, sessionStats })

  if (!data) return null

  const {
    contextUsed,
    contextMax,
    contextPercent,
    dashOffset,
    rows,
    total,
    hasContext,
    hasUsage,
    hasTokenSection,
  } = data

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          onMouseDown={(event) => {
            if (stopPropagation) event.stopPropagation()
          }}
          onClick={(event) => {
            if (stopPropagation) event.stopPropagation()
          }}
          onDoubleClick={(event) => {
            if (stopPropagation) event.stopPropagation()
          }}
          className={cn(
            "transition-colors",
            variant === "status"
              ? "flex items-center gap-1 hover:text-foreground"
              : "inline-flex h-[1.125rem] items-center gap-1 rounded-[0.375rem] border border-sidebar-border/60 bg-sidebar-accent/55 px-1.5 text-[0.625rem] font-medium leading-none text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground aria-expanded:bg-sidebar-accent aria-expanded:text-sidebar-foreground",
            className
          )}
        >
          {hasContext ? (
            <>
              {showIcon ? (
                <svg
                  aria-label={t("contextWindowUsageAria")}
                  className={variant === "status" ? "size-3.5" : "size-3"}
                  viewBox={`0 0 ${ICON_VIEWBOX} ${ICON_VIEWBOX}`}
                >
                  <circle
                    cx={ICON_CENTER}
                    cy={ICON_CENTER}
                    r={ICON_RADIUS}
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    opacity="0.25"
                  />
                  <circle
                    cx={ICON_CENTER}
                    cy={ICON_CENTER}
                    r={ICON_RADIUS}
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeDasharray={`${ICON_CIRCUMFERENCE} ${ICON_CIRCUMFERENCE}`}
                    strokeDashoffset={dashOffset}
                    style={{
                      transformOrigin: "center",
                      transform: "rotate(-90deg)",
                    }}
                    opacity="0.75"
                  />
                </svg>
              ) : null}
              <span>{formatContextWindowPercent(contextPercent)}</span>
            </>
          ) : (
            <>
              {showIcon ? (
                <Coins
                  className={variant === "status" ? "size-3.5" : "size-3"}
                />
              ) : null}
              <span>{formatTokenCount(total ?? 0)}</span>
            </>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent
        side={popoverSide}
        align="end"
        className="w-56 gap-2 p-3 text-xs"
      >
        {hasContext ? (
          <div
            className={`space-y-1 ${
              hasUsage ? "mb-0.5 border-b border-border pb-0.5" : ""
            }`}
          >
            <div className="flex items-center justify-between gap-2 text-xs font-medium whitespace-nowrap">
              <span>{t("contextWindow")}</span>
              <span className="tabular-nums shrink-0">
                {formatContextWindowPercent(contextPercent)}
              </span>
            </div>
            <div className="relative h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="absolute inset-y-0 left-0 bg-foreground/70"
                style={{ width: `${contextPercent ?? 0}%` }}
              />
            </div>
            <div className="flex items-center justify-between text-xs leading-none text-muted-foreground">
              <span>{t("usedMax")}</span>
              <span className="tabular-nums">
                {contextUsed == null || contextMax == null
                  ? "--"
                  : `${formatTokenCount(contextUsed)} / ${formatTokenCount(contextMax)}`}
              </span>
            </div>
          </div>
        ) : null}
        {hasTokenSection ? (
          <>
            <div className="mb-0 mt-0.5 text-xs leading-none font-medium">
              {t("tokenUsage")}
            </div>
            <div className="space-y-0">
              {rows.map((row) => (
                <div
                  key={row.key}
                  className={`flex items-center justify-between py-0.5 text-xs leading-none ${
                    row.key === "total"
                      ? "mt-0.5 border-t border-border pt-0.5 font-medium"
                      : "text-muted-foreground"
                  }`}
                >
                  <span>{t(row.key)}</span>
                  <span className="tabular-nums">
                    {formatTokenCount(row.value)}
                  </span>
                </div>
              ))}
            </div>
          </>
        ) : null}
      </PopoverContent>
    </Popover>
  )
}

export function StatusBarTokens(props: SessionUsageSourceProps) {
  return <SessionUsageButton {...props} variant="status" />
}

export function SessionUsageChip({
  className,
  popoverSide = "bottom",
  stopPropagation = true,
  showIcon = true,
  ...props
}: SessionUsageSourceProps &
  Pick<
    SessionUsageButtonProps,
    "className" | "popoverSide" | "stopPropagation" | "showIcon"
  >) {
  return (
    <SessionUsageButton
      {...props}
      variant="chip"
      className={className}
      popoverSide={popoverSide}
      stopPropagation={stopPropagation}
      showIcon={showIcon}
    />
  )
}
