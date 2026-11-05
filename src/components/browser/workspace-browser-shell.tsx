"use client"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { BrowserShell } from "./browser-shell"

export function WorkspaceBrowserShell() {
  const { openTab } = useAuxPanelContext()
  return (
    <BrowserShell
      onOpenOverview={() => {
        openTab("home")
      }}
    />
  )
}
