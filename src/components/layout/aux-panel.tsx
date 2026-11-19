"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2 } from "lucide-react"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { AuxPanelHome } from "./aux-panel-home"
import { TaskArtifactsTab } from "./aux-panel-artifacts-tab"
import { FileTreeTab } from "./aux-panel-file-tree-tab"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { isDesktop, onTransportChange } from "@/lib/transport"
import { isPluginPanelVisibleForConversation } from "@/lib/plugin-panel"
import { usePluginPanels } from "./use-plugin-panels"
import { PluginPanelFrame } from "./plugin-panel-frame"
import { AuxPanelNavigation } from "./plugin-panel-navigation"
import { CollapsedPluginRail } from "./collapsed-plugin-rail"

export function AuxPanel({
  railSlot = null,
}: {
  railSlot?: HTMLElement | null
}) {
  const { profile, status } = useIywAccount(),
    [revision, setRevision] = useState(0)
  useEffect(
    () => onTransportChange(() => setRevision((value) => value + 1)),
    []
  )
  return (
    <AuxWorkspace
      railSlot={railSlot}
      key={JSON.stringify([
        profile?.user_id,
        profile?.org_code,
        profile?.workspace_pending,
        status,
        revision,
      ])}
    />
  )
}

function AuxWorkspace({ railSlot }: { railSlot: HTMLElement | null }) {
  const t = useTranslations("Folder.auxPanel.tabs")
  const pluginText = useTranslations("PluginPanels")
  const { isOpen, activeTab } = useAuxPanelContext()
  const panels = usePluginPanels()
  const activeApp = panels.tabs.find(
    (tab) =>
      `app:${tab.id}` === activeTab &&
      isPluginPanelVisibleForConversation(tab, panels.conversationId)
  )
  const opening =
    panels.openingApp?.tabId === panels.tabId ? panels.openingApp : null
  const preparing =
    opening &&
    (activeApp?.pluginSlug !== opening.app.pluginSlug ||
      activeApp?.appKey !== opening.app.appKey)

  return (
    <>
      <CollapsedPluginRail slot={railSlot} panels={panels} />
      <aside
        hidden={!isOpen}
        className="group/aux-panel h-full min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground select-none"
        style={isOpen ? { display: "flex" } : undefined}
      >
        <AuxPanelNavigation label={t("label")} panels={panels} />
        <div className="relative flex min-h-0 flex-1 flex-col">
          {(activeTab === "home" ||
            (activeTab.startsWith("app:") && !activeApp)) && (
            <AuxPanelHome panels={panels} />
          )}
          {activeTab === "artifacts" && <TaskArtifactsTab />}
          {activeTab === "file_tree" && <FileTreeTab />}
          {panels.tabs.map((tab) => (
            <PluginPanelFrame
              key={tab.id}
              tab={tab}
              visible={isOpen && activeApp?.id === tab.id}
              sync={panels.updates.states[tab.id]}
              refresh={panels.updates.refresh}
              moving={panels.detachingId === tab.id}
              onClose={() => panels.close(tab.id)}
              onRetry={() => panels.retry(tab.id)}
              onDetach={isDesktop() ? () => panels.detach(tab) : undefined}
              onOpenComplete={() => panels.finishOpening(tab)}
            />
          ))}
          {preparing && opening && (
            <div
              className="absolute inset-0 z-10 flex flex-col items-center justify-center gap-3 bg-sidebar text-sm text-muted-foreground"
              role="status"
              aria-live="polite"
            >
              <Loader2 className="size-5 animate-spin" aria-hidden="true" />
              <span>
                {pluginText("pluginOpening", {
                  name: opening.app.label,
                })}
              </span>
            </div>
          )}
        </div>
      </aside>
    </>
  )
}
