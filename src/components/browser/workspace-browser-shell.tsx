"use client"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { BrowserShell } from "./browser-shell"

export function WorkspaceBrowserShell() {
  const { openTab } = useAuxPanelContext()
  const { closeBrowser } = useBrowser()
  return (
    <BrowserShell
      onOpenOverview={() => {
        void closeBrowser()
        openTab("home")
      }}
    />
  )
}
