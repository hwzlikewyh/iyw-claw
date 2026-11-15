"use client"
import { useState } from "react"
import {
  Files,
  House,
  PackageOpen,
  PanelRightClose,
  Plus,
  X,
  SquareArrowOutUpRight,
  Loader2,
} from "lucide-react"
import { useTranslations } from "next-intl"
import {
  useAuxPanelContext,
  type AuxPanelTab,
} from "@/contexts/aux-panel-context"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import type { usePluginPanels, PluginPanelTab } from "./use-plugin-panels"
import { usePluginTabDrag } from "./use-plugin-tab-drag"
import { isDesktop } from "@/lib/transport"
import { toast } from "sonner"
import { PluginIcon } from "./plugin-icon"

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
            <AppTab
              key={tab.id}
              tab={tab}
              close={panels.close}
              detach={panels.detach}
              moving={panels.detachingId === tab.id}
            />
          ))}
      </div>
      <Button
        variant="ghost"
        size="icon-xs"
        title={pluginText("newTab")}
        aria-label={pluginText("newTab")}
        onClick={() => {
          setActiveTab("home")
          panels.refresh()
        }}
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
  detach,
  moving,
}: {
  tab: PluginPanelTab
  close: (id: string) => Promise<boolean>
  detach: (tab: PluginPanelTab) => Promise<boolean>
  moving: boolean
}) {
  const { activeTab, setActiveTab } = useAuxPanelContext()
  const pluginText = useTranslations("PluginPanels")
  const { closing, closePanel } = useAppTabClose(tab.id, close)
  const move = () => {
    if (!closing && !moving)
      void detach(tab).then((success) => {
        if (!success) toast.error(pluginText("moveFailed"))
      })
  }
  const { consumeDrag, ...drag } = usePluginTabDrag(move)
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          key={tab.id}
          className={cn(
            "flex h-8 shrink-0 items-center rounded-md",
            activeTab === `app:${tab.id}` && "bg-background"
          )}
        >
          <button
            {...drag}
            disabled={moving || closing}
            type="button"
            className="flex max-w-36 items-center gap-1.5 truncate px-2 text-xs"
            aria-current={activeTab === `app:${tab.id}` ? "page" : undefined}
            onClick={() => {
              if (!consumeDrag()) setActiveTab(`app:${tab.id}`)
            }}
          >
            {moving ? (
              <Loader2 className="size-3.5 shrink-0 animate-spin" />
            ) : (
              <PluginIcon
                slug={tab.pluginSlug}
                label={tab.label}
                className="size-5"
              />
            )}
            <span className="truncate">
              {moving ? pluginText("openingWindow") : tab.label}
            </span>
          </button>
          <button
            type="button"
            disabled={closing || moving}
            aria-busy={closing}
            className="p-1 hover:text-destructive"
            aria-label={pluginText("closeApp", { name: tab.label })}
            onClick={() => void closePanel()}
          >
            <X className="size-3" />
          </button>
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent>
        {isDesktop() && (
          <ContextMenuItem disabled={closing || moving} onSelect={move}>
            <SquareArrowOutUpRight className="size-3.5" />
            {pluginText("detachApp")}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
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
