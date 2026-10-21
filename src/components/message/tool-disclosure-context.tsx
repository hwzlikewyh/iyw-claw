"use client"

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  useEffect,
} from "react"
import type { ReactNode } from "react"

const TOOL_SETTLE_MS = 650

const ToolDisclosureContext = createContext<{
  choices: ReadonlyMap<string, boolean>
  setToolOpen: (toolCallId: string, open: boolean) => void
  reading: boolean
} | null>(null)

export function ToolDisclosureScope({
  children,
  reading = false,
}: {
  children: ReactNode
  reading?: boolean
}) {
  const [choices, setChoices] = useState<ReadonlyMap<string, boolean>>(
    new Map()
  )
  const setToolOpen = useCallback((toolCallId: string, open: boolean) => {
    setChoices((current) => {
      if (current.get(toolCallId) === open) return current
      const next = new Map(current)
      next.set(toolCallId, open)
      return next
    })
  }, [])
  const value = useMemo(
    () => ({ choices, setToolOpen, reading }),
    [choices, setToolOpen, reading]
  )
  return (
    <ToolDisclosureContext.Provider value={value}>
      {children}
    </ToolDisclosureContext.Provider>
  )
}

export function useToolDisclosure(toolCallId: string, preferredOpen = false) {
  const scope = useContext(ToolDisclosureContext)
  const [localOpen, setLocalOpen] = useState<boolean | null>(null)
  const [autoOpen, setAutoOpen] = useState(preferredOpen)
  const reading = scope?.reading ?? false
  useEffect(() => {
    if (reading) return
    const timer = window.setTimeout(
      () => setAutoOpen(preferredOpen),
      TOOL_SETTLE_MS
    )
    return () => window.clearTimeout(timer)
  }, [preferredOpen, reading])
  const setOpen = (open: boolean) => {
    if (scope) scope.setToolOpen(toolCallId, open)
    else setLocalOpen(open)
  }
  // 虚拟列表卸载离屏卡片时，仅保留工具 ID 和开关状态。
  const choice = scope ? scope.choices.get(toolCallId) : localOpen
  return [choice ?? (preferredOpen || autoOpen), setOpen] as const
}
