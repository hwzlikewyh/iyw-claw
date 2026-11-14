"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { AuxPanelHome } from "./aux-panel-home"
import { TaskArtifactsTab } from "./aux-panel-artifacts-tab"
import { FileTreeTab } from "./aux-panel-file-tree-tab"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { onTransportChange } from "@/lib/transport"
import { usePluginPanels } from "./use-plugin-panels"
import { PluginPanelFrame } from "./plugin-panel-frame"
import { AuxPanelNavigation } from "./plugin-panel-navigation"
import { CollapsedPluginRail } from "./collapsed-plugin-rail"
import { PluginPresentationProvider } from "./plugin-presentation"

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
    <PluginPresentationProvider
      key={JSON.stringify([
        profile?.user_id,
        profile?.org_code,
        profile?.workspace_pending,
        status,
        revision,
      ])}
    >
      <AuxWorkspace railSlot={railSlot} />
    </PluginPresentationProvider>
  )
}

function AuxWorkspace({ railSlot }: { railSlot: HTMLElement | null }) {
  const t = useTranslations("Folder.auxPanel.tabs")
  const { isOpen, activeTab } = useAuxPanelContext()
  const panels = usePluginPanels()
  const activeApp = panels.tabs.find(
    (tab) =>
      `app:${tab.id}` === activeTab &&
      tab.conversationId === panels.conversationId
  )

  return (
    <>
      <CollapsedPluginRail slot={railSlot} panels={panels} />
      <aside
        hidden={!isOpen}
        className="group/aux-panel h-full min-h-0 min-w-0 flex-col overflow-hidden bg-sidebar text-sidebar-foreground select-none"
        style={isOpen ? { display: "flex" } : undefined}
      >
        <AuxPanelNavigation label={t("label")} panels={panels} />
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
          />
        ))}
      </aside>
    </>
  )
}
