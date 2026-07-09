"use client"

import { createContext, useCallback, useContext, useState } from "react"
import {
  loadSectionOrder,
  loadShowCompleted,
  loadSortMode,
  saveSectionOrder,
  saveShowCompleted,
  saveSortMode,
  type SidebarSectionOrder,
  type SidebarSortMode,
} from "@/lib/sidebar-view-mode-storage"

type SidebarViewOptionsContextValue = {
  showCompleted: boolean
  setShowCompleted: (value: boolean) => void
  sortMode: SidebarSortMode
  setSortMode: (value: SidebarSortMode) => void
  sectionOrder: SidebarSectionOrder
  setSectionOrder: (value: SidebarSectionOrder) => void
}

const SidebarViewOptionsContext =
  createContext<SidebarViewOptionsContextValue | null>(null)

export function SidebarViewOptionsProvider({
  children,
}: {
  children: React.ReactNode
}) {
  const [showCompleted, setShowCompletedState] = useState(loadShowCompleted)
  const [sortMode, setSortModeState] = useState<SidebarSortMode>(loadSortMode)
  const [sectionOrder, setSectionOrderState] =
    useState<SidebarSectionOrder>(loadSectionOrder)

  const setShowCompleted = useCallback((value: boolean) => {
    setShowCompletedState(value)
    saveShowCompleted(value)
  }, [])

  const setSortMode = useCallback((value: SidebarSortMode) => {
    setSortModeState(value)
    saveSortMode(value)
  }, [])

  const setSectionOrder = useCallback((value: SidebarSectionOrder) => {
    setSectionOrderState(value)
    saveSectionOrder(value)
  }, [])

  return (
    <SidebarViewOptionsContext.Provider
      value={{
        showCompleted,
        setShowCompleted,
        sortMode,
        setSortMode,
        sectionOrder,
        setSectionOrder,
      }}
    >
      {children}
    </SidebarViewOptionsContext.Provider>
  )
}

export function useSidebarViewOptions() {
  const context = useContext(SidebarViewOptionsContext)
  if (!context) {
    throw new Error(
      "useSidebarViewOptions must be used within SidebarViewOptionsProvider"
    )
  }
  return context
}
