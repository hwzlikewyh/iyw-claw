"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import {
  loadPersistedPanelState,
  savePersistedPanelState,
} from "@/lib/panel-state-storage"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { isDetachedConversationWindow } from "@/lib/conversation-window-state"
import { peekConversationPanels } from "@/lib/conversation-window-panels"

export type AuxPanelTab = "home" | "file_tree" | "artifacts" | `app:${string}`

const STORAGE_KEY = "workspace:right-sidebar"

const DEFAULT_WIDTH = 320
const MIN_WIDTH = 200
const MAX_WIDTH = 900
const DEFAULT_IS_OPEN = false

interface AuxPanelContextValue {
  isOpen: boolean
  restored: boolean
  width: number
  minWidth: number
  maxWidth: number
  activeTab: AuxPanelTab
  toggle: () => void
  /** 显式设置面板开关，供右侧入口协调布局。 */
  setOpen: (open: boolean) => void
  setWidth: (w: number) => void
  setActiveTab: (tab: AuxPanelTab) => void
  openTab: (tab: AuxPanelTab) => void
  pendingRevealPath: string | null
  revealInFileTree: (path: string) => void
  consumePendingRevealPath: () => void
}

const AuxPanelContext = createContext<AuxPanelContextValue | null>(null)

export function useAuxPanelContext() {
  const ctx = useContext(AuxPanelContext)
  if (!ctx) {
    throw new Error("useAuxPanelContext must be used within AuxPanelProvider")
  }
  return ctx
}

function clampWidth(width: number) {
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, width))
}

interface AuxPanelProviderProps {
  children: ReactNode
}

export function AuxPanelProvider({ children }: AuxPanelProviderProps) {
  const storageKey = STORAGE_KEY
  const { activeFolderId } = useActiveFolder()
  const [isOpen, setIsOpen] = useState(DEFAULT_IS_OPEN)
  const [width, setWidthState] = useState(DEFAULT_WIDTH)
  const [restored, setRestored] = useState(false)
  const [activeTab, setActiveTab] = useState<AuxPanelTab>("home")
  const [pendingRevealPath, setPendingRevealPath] = useState<string | null>(
    null
  )

  const toggle = useCallback(() => setIsOpen((prev) => !prev), [])

  const setOpen = useCallback((open: boolean) => setIsOpen(open), [])

  const setWidth = useCallback((w: number) => {
    setWidthState(clampWidth(w))
  }, [])

  const openTab = useCallback((tab: AuxPanelTab) => {
    setActiveTab(tab)
    setIsOpen(true)
  }, [])

  const revealInFileTree = useCallback((path: string) => {
    setPendingRevealPath(path)
    setActiveTab("file_tree")
    setIsOpen(true)
  }, [])

  const consumePendingRevealPath = useCallback(() => {
    setPendingRevealPath(null)
  }, [])

  useEffect(() => {
    const stored = loadPersistedPanelState(storageKey)
    // 普通新草稿只恢复宽度；独立会话窗口默认带上右侧工具栏。
    const incoming = peekConversationPanels()
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setIsOpen(incoming?.isOpen ?? isDetachedConversationWindow())
    setWidthState(clampWidth(incoming?.width ?? stored?.width ?? DEFAULT_WIDTH))
    if (
      incoming &&
      ["home", "file_tree", "artifacts"].includes(incoming.activeTab)
    )
      setActiveTab(incoming.activeTab as AuxPanelTab)
    setRestored(true)
  }, [storageKey])

  useEffect(() => {
    if (!restored) return
    savePersistedPanelState(storageKey, { isOpen, width })
  }, [isOpen, restored, storageKey, width])

  // Reset pending reveal path when the active folder changes; file tree
  // state is content-driven by the workspace contexts and will refetch
  // naturally via its folder-path dependency.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setPendingRevealPath(null)
  }, [activeFolderId])

  const value = useMemo(
    () => ({
      isOpen,
      restored,
      width,
      minWidth: MIN_WIDTH,
      maxWidth: MAX_WIDTH,
      activeTab,
      toggle,
      setOpen,
      setWidth,
      setActiveTab,
      openTab,
      pendingRevealPath,
      revealInFileTree,
      consumePendingRevealPath,
    }),
    [
      isOpen,
      restored,
      width,
      activeTab,
      toggle,
      setOpen,
      setWidth,
      openTab,
      pendingRevealPath,
      revealInFileTree,
      consumePendingRevealPath,
    ]
  )

  return (
    <AuxPanelContext.Provider value={value}>
      {children}
    </AuxPanelContext.Provider>
  )
}
