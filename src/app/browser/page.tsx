"use client"

import { BrowserShell } from "@/components/browser/browser-shell"
import { BrowserProvider } from "@/contexts/browser-context"

export default function BrowserPage() {
  return (
    <BrowserProvider defaultOpen>
      <main className="fixed inset-0 overflow-hidden bg-background text-foreground">
        <BrowserShell kind="detached" />
      </main>
    </BrowserProvider>
  )
}
