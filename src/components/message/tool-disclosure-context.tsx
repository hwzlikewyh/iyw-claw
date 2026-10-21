"use client"

import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react"
import type { ReactNode } from "react"

const ToolDisclosureContext = createContext<{
  openTools: ReadonlySet<string>
  setToolOpen: (toolCallId: string, open: boolean) => void
} | null>(null)

export function ToolDisclosureScope({ children }: { children: ReactNode }) {
  const [openTools, setOpenTools] = useState<ReadonlySet<string>>(new Set())
  const setToolOpen = useCallback((toolCallId: string, open: boolean) => {
    setOpenTools((current) => {
      if (current.has(toolCallId) === open) return current
      const next = new Set(current)
      if (open) next.add(toolCallId)
      else next.delete(toolCallId)
      return next
    })
  }, [])
  const value = useMemo(
    () => ({ openTools, setToolOpen }),
    [openTools, setToolOpen]
  )
  return (
    <ToolDisclosureContext.Provider value={value}>
      {children}
    </ToolDisclosureContext.Provider>
  )
}

export function useToolDisclosure(toolCallId: string) {
  const scope = useContext(ToolDisclosureContext)
  const [localOpen, setLocalOpen] = useState(false)
  const setOpen = (open: boolean) => {
    if (scope) scope.setToolOpen(toolCallId, open)
    else setLocalOpen(open)
  }
  // 虚拟列表卸载离屏卡片时，仅保留工具 ID 和开关状态。
  return [scope ? scope.openTools.has(toolCallId) : localOpen, setOpen] as const
}
