"use client"

import { useLayoutEffect, useRef, type ReactNode, type RefObject } from "react"

type MessagePhase = "persisted" | "optimistic" | "streaming"
type MessageRole = "user" | "assistant" | "system"

const MAX_TRACKED_ENTRANCES = 2000
const TRIMMED_ENTRANCES = 500
const playedEntrances = new Set<string>()
const streamedMessages = new Set<string>()

function remember(set: Set<string>, key: string): boolean {
  if (set.has(key)) return false
  set.add(key)
  if (set.size > MAX_TRACKED_ENTRANCES) {
    for (const oldKey of Array.from(set).slice(0, TRIMMED_ENTRANCES)) {
      set.delete(oldKey)
    }
  }
  return true
}

function playEntrance(
  element: HTMLElement,
  offset: number,
  duration: number
): Animation | undefined {
  if (
    document.hidden ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  )
    return
  return element.animate(
    [
      { opacity: 0, transform: `translateY(${offset}px)` },
      { opacity: 1, transform: "translateY(0)" },
    ],
    {
      duration,
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
      fill: "none",
    }
  )
}

function useEntrance(
  ref: RefObject<HTMLDivElement | null>,
  key: string,
  animate: boolean,
  record: boolean,
  offset: number,
  duration: number
): void {
  useLayoutEffect(() => {
    if (!record || !remember(playedEntrances, key)) return
    if (!animate || !ref.current) return
    const animation = playEntrance(ref.current, offset, duration)
    const stopWhenHidden = () => {
      if (document.hidden) animation?.cancel()
    }
    document.addEventListener("visibilitychange", stopWhenHidden)
    return () => {
      animation?.cancel()
      document.removeEventListener("visibilitychange", stopWhenHidden)
    }
  }, [animate, duration, key, offset, record, ref])
}

interface MessageEntranceProps {
  conversationId: number
  messageId: string
  role: MessageRole
  phase: MessagePhase
  enabled: boolean
  children: ReactNode
}

interface OnceEntranceProps {
  entranceKey: string
  animate: boolean
  children: ReactNode
  offset?: number
  duration?: number
}

export function OnceEntrance({
  entranceKey,
  animate,
  children,
  offset = 1,
  duration = 120,
}: OnceEntranceProps) {
  const ref = useRef<HTMLDivElement>(null)
  useEntrance(ref, entranceKey, animate, true, offset, duration)
  return <div ref={ref}>{children}</div>
}

export function MessageEntrance({
  conversationId,
  messageId,
  role,
  phase,
  enabled,
  children,
}: MessageEntranceProps) {
  const ref = useRef<HTMLDivElement>(null)
  const logicalKey = `${conversationId}:${role}:${messageId}`
  const eligible = phase === "optimistic" || phase === "streaming"
  const offset = 2
  const duration = 140
  useLayoutEffect(() => {
    if (phase === "streaming" && enabled) {
      remember(streamedMessages, logicalKey)
    }
  }, [enabled, logicalKey, phase])
  useEntrance(
    ref,
    `row:${logicalKey}`,
    enabled && eligible,
    true,
    offset,
    duration
  )
  return <div ref={ref}>{children}</div>
}

interface CompletionEntranceProps {
  conversationId: number
  messageId: string
  enabled: boolean
  complete: boolean
  children: ReactNode
}

export function CompletionEntrance({
  conversationId,
  messageId,
  enabled,
  complete,
  children,
}: CompletionEntranceProps) {
  const ref = useRef<HTMLDivElement>(null)
  const logicalKey = `${conversationId}:assistant:${messageId}`
  const streamed = streamedMessages.has(logicalKey)
  useEntrance(
    ref,
    `actions:${logicalKey}`,
    enabled && complete && streamed,
    !enabled || complete,
    2,
    160
  )
  return <div ref={ref}>{children}</div>
}
