"use client"

import { Files, PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { cn } from "@/lib/utils"
import { TaskArtifactsTab } from "./aux-panel-artifacts-tab"
import { FileTreeTab } from "./aux-panel-file-tree-tab"

export function AuxPanel() {
  const t = useTranslations("Folder.auxPanel.tabs")
  const { isOpen, activeTab, setActiveTab } = useAuxPanelContext()

  if (!isOpen) return null
  const selected = activeTab === "artifacts" ? "artifacts" : "file_tree"

  return (
    <aside className="group/aux-panel flex h-full min-h-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground select-none">
      <nav
        className="grid h-10 shrink-0 grid-cols-2 border-b p-1"
        aria-label={t("label")}
      >
        {[
          { id: "file_tree" as const, label: t("files"), icon: Files },
          {
            id: "artifacts" as const,
            label: t("artifacts"),
            icon: PackageOpen,
          },
        ].map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            type="button"
            onClick={() => setActiveTab(id)}
            className={cn(
              "flex items-center justify-center gap-1.5 rounded-md text-xs font-medium text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground",
              selected === id && "bg-background text-foreground shadow-xs"
            )}
            aria-current={selected === id ? "page" : undefined}
          >
            <Icon className="size-3.5" />
            {label}
          </button>
        ))}
      </nav>
      {selected === "artifacts" ? <TaskArtifactsTab /> : <FileTreeTab />}
    </aside>
  )
}
