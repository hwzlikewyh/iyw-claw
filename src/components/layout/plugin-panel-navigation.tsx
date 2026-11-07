"use client"
import { useState } from "react"
import {
  Files,
  House,
  PackageOpen,
  PanelRightClose,
  Plus,
  X,
  AppWindow,
} from "lucide-react"
import { useTranslations } from "next-intl"
import {
  useAuxPanelContext,
  type AuxPanelTab,
} from "@/contexts/aux-panel-context"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import type { usePluginPanels, PluginPanelTab } from "./use-plugin-panels"

export function AuxPanelNavigation({
  label,
  panels,
}: {
  label: string
  panels: ReturnType<typeof usePluginPanels>
}) {
  const t = useTranslations("Folder.auxPanel.tabs")
  const tTitleBar = useTranslations("Folder.folderTitleBar")
  const { setActiveTab, toggle } = useAuxPanelContext()
  const pluginText = useTranslations("PluginPanels")
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
      <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
        {tabs.map((tab) => (
          <StandardTab key={tab.id} {...tab} />
        ))}
        {panels.tabs
          .filter((tab) => tab.conversationId === panels.conversationId)
          .map((tab) => (
            <AppTab key={tab.id} tab={tab} close={panels.close} />
          ))}
      </div>
      <Button
        variant="ghost"
        size="icon-xs"
        title={pluginText("newTab")}
        aria-label={pluginText("newTab")}
        onClick={() => setActiveTab("home")}
      >
        <Plus className="size-3.5" />
      </Button>
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

function StandardTab({
  id,
  label,
  icon: Icon,
}: {
  id: AuxPanelTab
  label: string
  icon: typeof House
}) {
  const { activeTab, setActiveTab } = useAuxPanelContext()
  return (
    <button
      key={id}
      type="button"
      onClick={() => setActiveTab(id)}
      className={cn(
        "flex h-8 shrink-0 items-center justify-center gap-1.5 rounded-md px-2 text-xs font-medium text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:outline focus-visible:outline-ring",
        activeTab === id && "bg-background text-foreground"
      )}
      aria-current={activeTab === id ? "page" : undefined}
    >
      <Icon className="size-3.5" />
      {label}
    </button>
  )
}
function AppTab({
  tab,
  close,
}: {
  tab: PluginPanelTab
  close: (id: string) => Promise<boolean>
}) {
  const { activeTab, setActiveTab } = useAuxPanelContext()
  const pluginText = useTranslations("PluginPanels")
  const { closing, closePanel } = useAppTabClose(tab.id, close)
  return (
    <div
      key={tab.id}
      className={cn(
        "flex h-8 shrink-0 items-center rounded-md",
        activeTab === `app:${tab.id}` && "bg-background"
      )}
    >
      <button
        type="button"
        className="flex max-w-36 items-center gap-1.5 truncate px-2 text-xs"
        aria-current={activeTab === `app:${tab.id}` ? "page" : undefined}
        onClick={() => setActiveTab(`app:${tab.id}`)}
      >
        <AppWindow className="size-3.5 shrink-0" />
        <span className="truncate">{tab.label}</span>
      </button>
      <button
        type="button"
        disabled={closing}
        aria-busy={closing}
        className="p-1 hover:text-destructive"
        aria-label={pluginText("closeApp", { name: tab.label })}
        onClick={() => void closePanel()}
      >
        <X className="size-3" />
      </button>
    </div>
  )
}

function useAppTabClose(id: string, close: (id: string) => Promise<boolean>) {
  const t = useTranslations("PluginPanels")
  const [closing, setClosing] = useState(false)
  const closePanel = async () => {
    if (closing || !window.confirm(t("closeConfirm"))) return
    setClosing(true)
    try {
      if (!(await close(id))) window.alert(t("closeFailed"))
    } catch {
      window.alert(t("closeFailed"))
    } finally {
      setClosing(false)
    }
  }
  return { closing, closePanel }
}
