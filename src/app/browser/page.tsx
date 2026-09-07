"use client"

import { useEffect } from "react"
import { BrowserShell } from "@/components/browser/browser-shell"
import { BrowserProvider } from "@/contexts/browser-context"
import {
  readBrowserVisibility,
  useBrowserVisibility,
} from "@/hooks/use-browser-visibility"
import { closeHiddenBrowserWindow } from "@/lib/browser-window-visibility"
import { getCurrentWindow } from "@/lib/platform"

export default function BrowserPage() {
  const visible = useBrowserVisibility()
  useEffect(() => {
    if (readBrowserVisibility()) return
    void hideBrowserWindow().catch((error) => {
      console.error("[Browser] failed to hide detached window", error)
    })
  }, [visible])

  return (
    <BrowserProvider defaultOpen autoOpenUserActionWindow={false}>
      <main
        hidden={!visible}
        className="fixed inset-0 overflow-hidden bg-background text-foreground"
      >
        <BrowserShell kind="detached" />
      </main>
    </BrowserProvider>
  )
}

async function hideBrowserWindow() {
  const window = await getCurrentWindow()
  if (!window || readBrowserVisibility()) return
  await closeHiddenBrowserWindow(window.label)
}
