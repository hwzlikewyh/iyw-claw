"use client"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { FileTreeTab } from "./aux-panel-file-tree-tab"

export function AuxPanel() {
  const { isOpen } = useAuxPanelContext()

  if (!isOpen) return null

  return (
    <aside className="group/aux-panel flex h-full min-h-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground select-none">
      <FileTreeTab />
    </aside>
  )
}
