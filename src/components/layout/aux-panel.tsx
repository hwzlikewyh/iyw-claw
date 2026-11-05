"use client"

import { Files, House, PackageOpen, PanelRightClose } from "lucide-react"
import { useTranslations } from "next-intl"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { AuxPanelHome } from "./aux-panel-home"
import { TaskArtifactsTab } from "./aux-panel-artifacts-tab"
import { FileTreeTab } from "./aux-panel-file-tree-tab"

export function AuxPanel() {
  const t = useTranslations("Folder.auxPanel.tabs")
  const { isOpen, activeTab } = useAuxPanelContext()

  if (!isOpen) return null

  return (
    <aside className="group/aux-panel flex h-full min-h-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground select-none">
      <AuxPanelNavigation label={t("label")} />
      {activeTab === "home" && <AuxPanelHome />}
      {activeTab === "artifacts" && <TaskArtifactsTab />}
      {activeTab === "file_tree" && <FileTreeTab />}
    </aside>
  )
}

function AuxPanelNavigation({ label }: { label: string }) {
  const t = useTranslations("Folder.auxPanel.tabs")
  const tTitleBar = useTranslations("Folder.folderTitleBar")
  const { activeTab, setActiveTab, toggle } = useAuxPanelContext()
  const tabs = [
    { id: "home" as const, label: t("home"), icon: House },
    { id: "file_tree" as const, label: t("files"), icon: Files },
    { id: "artifacts" as const, label: t("artifacts"), icon: PackageOpen },
  ]
  return (
    <nav
      className="flex h-10 shrink-0 items-center gap-1 border-b p-1"
      aria-label={label}
    >
      {tabs.map(({ id, label, icon: Icon }) => (
        <button
          key={id}
          type="button"
          onClick={() => setActiveTab(id)}
          className={cn(
            "flex h-8 min-w-0 flex-1 items-center justify-center gap-1.5 rounded-md text-xs font-medium text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:outline focus-visible:outline-ring",
            activeTab === id && "bg-background text-foreground"
          )}
          aria-current={activeTab === id ? "page" : undefined}
        >
          <Icon className="size-3.5" />
          {label}
        </button>
      ))}
      <Button
        variant="ghost"
        size="icon"
        className="size-7 shrink-0"
        title={tTitleBar("toggleAuxPanel")}
        aria-label={tTitleBar("toggleAuxPanel")}
        onClick={toggle}
      >
        <PanelRightClose className="size-3.5" />
      </Button>
    </nav>
  )
}
