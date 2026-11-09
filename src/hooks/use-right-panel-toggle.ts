"use client"

import { useCallback } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"

export function useRightPanelToggle() {
  const { isOpen: auxOpen, setOpen, openTab } = useAuxPanelContext()
  const { isOpen: browserOpen, closeBrowser } = useBrowser()
  const { isOpen: pageOpen, closePanel } = useInteractivePage()
  const isOpen = auxOpen || browserOpen || pageOpen
  const toggle = useCallback(() => {
    closePanel()
    setOpen(false)
    if (isOpen) void closeBrowser()
    else openTab("home")
  }, [closeBrowser, closePanel, isOpen, openTab, setOpen])
  return { isOpen, toggle }
}
