"use client"

import { ArrowLeft, Files, PackageOpen, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { cn } from "@/lib/utils"
import { TaskArtifactsTab } from "./aux-panel-artifacts-tab"
import { FileTreeTab } from "./aux-panel-file-tree-tab"
import { TaskArtifactPreview } from "./task-artifact-preview"
import { Button } from "@/components/ui/button"

export function AuxPanel() {
  const t = useTranslations("Folder.auxPanel.tabs")
  const tArtifacts = useTranslations("Folder.taskArtifacts")
  const {
    isOpen,
    activeTab,
    setActiveTab,
    setOpen,
    artifactPreview,
    closeArtifactPreview,
  } = useAuxPanelContext()

  if (!isOpen) return null
  if (artifactPreview) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="flex h-10 shrink-0 items-center gap-1 border-b px-2">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={closeArtifactPreview}
            aria-label={tArtifacts("backToList")}
            title={tArtifacts("backToList")}
          >
            <ArrowLeft className="size-4" />
          </Button>
          <span className="min-w-0 flex-1 truncate text-xs font-medium">
            {artifactPreview.displayName}
          </span>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={() => {
              closeArtifactPreview()
              setOpen(false)
            }}
            aria-label={tArtifacts("backToList")}
            title={tArtifacts("backToList")}
          >
            <X className="size-4" />
          </Button>
        </div>
        <TaskArtifactPreview
          artifact={artifactPreview}
          className="min-h-0 flex-1"
          onOpenWorkspace={closeArtifactPreview}
        />
      </div>
    )
  }
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
