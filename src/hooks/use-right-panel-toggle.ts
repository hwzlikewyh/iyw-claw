"use client"

import { useCallback } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { useInteractivePage } from "@/contexts/interactive-page-context"
import { useBrowserVisibility } from "./use-browser-visibility"
import { isLocalDesktop } from "@/lib/platform"

export function useRightPanelToggle() {
  const { isOpen: auxOpen, setOpen, openTab } = useAuxPanelContext()
  const { isOpen: browserOpen, openBrowser, closeBrowser } = useBrowser()
  const { isOpen: pageOpen, closePanel } = useInteractivePage()
  const browserVisible = useBrowserVisibility()
  const isOpen = auxOpen || browserOpen || pageOpen
  const toggle = useCallback(() => {
    closePanel()
    setOpen(false)
    if (isOpen) void closeBrowser()
    else if (browserVisible && isLocalDesktop()) void openBrowser()
    else openTab("home")
  }, [
    browserVisible,
    closeBrowser,
    closePanel,
    isOpen,
    openBrowser,
    openTab,
    setOpen,
  ])
  return { isOpen, toggle }
}
