"use client"

import { memo, useEffect, useRef, useState, type RefObject } from "react"
import { Tooltip } from "radix-ui"
import { useTranslations } from "next-intl"
import { useStickToBottomContext } from "use-stick-to-bottom"
import type { MessageScrollContextValue } from "./message-scroll-context"
import { cn } from "@/lib/utils"

const BOTTOM_THRESHOLD_PX = 8
const READING_OFFSET_PX = 32
const PREVIEW_DELAY_MS = 120
const PREVIEW_SKIP_DELAY_MS = 100

/** 一条用户消息及其回复摘要，索引与虚拟列表保持一致。 */
export interface MessageNavEntry {
  threadIndex: number
  turnId: string
  ordinal: number
  label: string
  preview: string
}

interface ConversationMessageNavProps {
  entries: MessageNavEntry[]
  scrollApiRef: RefObject<MessageScrollContextValue | null>
}

function getCurrentTurn(viewport: HTMLElement, entries: MessageNavEntry[]) {
  const latest = entries[entries.length - 1]?.turnId
  const remaining =
    viewport.scrollHeight - viewport.clientHeight - viewport.scrollTop
  if (remaining <= BOTTOM_THRESHOLD_PX) return latest

  const readingTop = viewport.getBoundingClientRect().top + READING_OFFSET_PX
  const rows = viewport.querySelectorAll<HTMLElement>(
    "[data-message-nav-index]"
  )
  for (const row of rows) {
    if (row.getBoundingClientRect().bottom <= readingTop) continue
    const threadIndex = Number(row.dataset.messageNavIndex)
    let current = entries[0]?.turnId
    for (const entry of entries) {
      if (entry.threadIndex > threadIndex) break
      current = entry.turnId
    }
    return current
  }
  return latest
}

function useCurrentTurn(entries: MessageNavEntry[]) {
  const { scrollRef } = useStickToBottomContext()
  const [current, setCurrent] = useState(entries[entries.length - 1]?.turnId)

  useEffect(() => {
    const viewport = scrollRef.current
    if (!viewport) return
    let frame = 0
    const scheduleUpdate = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() =>
        setCurrent(getCurrentTurn(viewport, entries))
      )
    }
    scheduleUpdate()
    viewport.addEventListener("scroll", scheduleUpdate, { passive: true })
    const observer = new ResizeObserver(scheduleUpdate)
    observer.observe(viewport)
    return () => {
      cancelAnimationFrame(frame)
      observer.disconnect()
      viewport.removeEventListener("scroll", scheduleUpdate)
    }
  }, [entries, scrollRef])

  return current
}

function MessageNavPreview({ entry }: { entry: MessageNavEntry }) {
  return (
    <Tooltip.Portal>
      <Tooltip.Content
        side="right"
        align="start"
        sideOffset={4}
        alignOffset={-4}
        collisionPadding={12}
        className="z-50 w-80 max-w-[min(20rem,var(--radix-tooltip-content-available-width))] rounded-lg border bg-popover p-3 text-sm leading-5 text-popover-foreground shadow-md"
      >
        <p className="line-clamp-2 whitespace-pre-line wrap-anywhere font-medium">
          {entry.label}
        </p>
        {entry.preview && (
          <p className="mt-2 line-clamp-3 whitespace-pre-line wrap-anywhere text-muted-foreground">
            {entry.preview}
          </p>
        )}
      </Tooltip.Content>
    </Tooltip.Portal>
  )
}

function MessageNavMarker({
  entry,
  active,
  scrollApiRef,
}: {
  entry: MessageNavEntry
  active: boolean
  scrollApiRef: ConversationMessageNavProps["scrollApiRef"]
}) {
  const t = useTranslations("Folder.chat.messageNav")
  const label = entry.label || t("collapsedSummary", { count: entry.ordinal })
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          aria-current={active ? "location" : undefined}
          onClick={() =>
            scrollApiRef.current?.scrollToIndex(entry.threadIndex, {
              align: "start",
              smooth: !window.matchMedia("(prefers-reduced-motion: reduce)")
                .matches,
            })
          }
          className="group flex h-2.5 w-10 shrink-0 items-center px-2 outline-none focus-visible:rounded-sm focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset [@media(pointer:coarse)]:h-8 [@media(pointer:coarse)]:w-11"
        >
          <span
            aria-hidden="true"
            className={cn(
              "h-0.5 w-3 shrink-0 rounded-full bg-muted-foreground/35 transition-[width,background-color] duration-150 group-hover:w-8 group-hover:bg-foreground/80 group-focus-visible:w-8 group-focus-visible:bg-foreground/80 group-data-[state=delayed-open]:w-8 group-data-[state=instant-open]:w-8 motion-reduce:transition-none",
              active && "w-5 bg-foreground/70"
            )}
          />
        </button>
      </Tooltip.Trigger>
      <MessageNavPreview entry={{ ...entry, label }} />
    </Tooltip.Root>
  )
}

export const ConversationMessageNav = memo(function ConversationMessageNav({
  entries,
  scrollApiRef,
}: ConversationMessageNavProps) {
  const t = useTranslations("Folder.chat.messageNav")
  const current = useCurrentTurn(entries)
  const railRef = useRef<HTMLElement>(null)

  useEffect(() => {
    const rail = railRef.current
    const marker = rail?.querySelector<HTMLElement>('[aria-current="location"]')
    if (!rail || !marker) return
    rail.scrollTop = marker.offsetTop - rail.clientHeight / 2
  }, [current])

  if (entries.length === 0) return null
  return (
    <Tooltip.Provider
      delayDuration={PREVIEW_DELAY_MS}
      skipDelayDuration={PREVIEW_SKIP_DELAY_MS}
    >
      <nav
        ref={railRef}
        aria-label={t("title")}
        className="absolute start-1 top-1/2 z-20 flex max-h-[min(20rem,50%)] -translate-y-1/2 flex-col overflow-y-auto overscroll-contain py-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {entries.map((entry) => (
          <MessageNavMarker
            key={entry.turnId}
            entry={entry}
            active={entry.turnId === current}
            scrollApiRef={scrollApiRef}
          />
        ))}
      </nav>
    </Tooltip.Provider>
  )
})
