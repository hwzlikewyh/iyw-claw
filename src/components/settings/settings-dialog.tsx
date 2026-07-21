"use client"

import { Suspense, useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { AcpAgentSettings } from "@/components/settings/acp-agent-settings"
import { AppearanceSettings } from "@/components/settings/appearance-settings"
import { ChatChannelSettings } from "@/components/settings/chat-channel-settings"
import { GeneralSettings } from "@/components/settings/general-settings"
import { LogsSettings } from "@/components/settings/logs-settings"
import { McpSettings } from "@/components/settings/mcp-settings"
import { SkillPacksSettings } from "@/components/settings/skill-packs-settings"
import { QuickMessagesSettings } from "@/components/settings/quick-messages-settings"
import { ShortcutSettings } from "@/components/settings/shortcut-settings"
import { SystemNetworkSettings } from "@/components/settings/system-network-settings"
import { UsageSettings } from "@/components/settings/usage-settings"
import { UserMemorySettings } from "@/components/settings/user-memory-settings"
import { VersionControlSettings } from "@/components/settings/version-control-settings"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import { SettingsShell } from "@/components/settings/settings-shell"
import { isDesktop, getShellTransport } from "@/lib/transport"
import {
  OPEN_SETTINGS_DIALOG_EVENT,
  normalizeSettingsSection,
  settingsPathToSection,
  settingsSectionToNavPath,
  type OpenSettingsDialogDetail,
} from "@/lib/settings-navigation"
import type { AgentType } from "@/lib/types"

type SettingsDialogState = OpenSettingsDialogDetail

interface NativeSettingsPayload {
  section?: string | null
  agentType?: AgentType | null
}

function SettingsDialogBody({ section, agentType }: SettingsDialogState) {
  switch (section) {
    case "appearance":
      return <AppearanceSettings />
    case "agents":
      return <AcpAgentSettings initialAgentType={agentType} />
    case "mcp":
      return <McpSettings />
    case "experts":
      return <SkillPacksSettings initialCategory="experts" />
    case "office-tools":
      return <SkillPacksSettings initialCategory="office-tools" />
    case "internet-tools":
      return <SkillPacksSettings initialCategory="internet-tools" />
    case "codex-native":
      return <SkillPacksSettings initialCategory="codex-native" />
    case "skills":
      return <SkillPacksSettings />
    case "quick-messages":
      return <QuickMessagesSettings />
    case "usage":
      return <UsageSettings />
    case "user-memory":
      return <UserMemorySettings />
    case "shortcuts":
      return <ShortcutSettings />
    case "version-control":
      return <VersionControlSettings />
    case "chat-channels":
      return <ChatChannelSettings />
    case "system":
      return <SystemNetworkSettings />
    case "logs":
      return <LogsSettings />
    case "general":
    case "model-providers":
    default:
      return <GeneralSettings />
  }
}

export function SettingsDialog() {
  const t = useTranslations("SettingsShell")
  const tPages = useTranslations("SettingsPages")
  const [open, setOpen] = useState(false)
  const [state, setState] = useState<SettingsDialogState>({
    section: "appearance",
    agentType: null,
  })

  const openSettings = useCallback((detail?: NativeSettingsPayload | null) => {
    setState({
      section: normalizeSettingsSection(detail?.section),
      agentType: detail?.agentType ?? null,
    })
    setOpen(true)
  }, [])

  useEffect(() => {
    const handleDomRequest = (event: Event) => {
      const customEvent = event as CustomEvent<OpenSettingsDialogDetail>
      customEvent.preventDefault()
      openSettings(customEvent.detail)
    }

    window.addEventListener(OPEN_SETTINGS_DIALOG_EVENT, handleDomRequest)
    return () => {
      window.removeEventListener(OPEN_SETTINGS_DIALOG_EVENT, handleDomRequest)
    }
  }, [openSettings])

  useEffect(() => {
    if (!isDesktop()) return

    let dispose: (() => void) | null = null
    let cancelled = false

    void getShellTransport()
      .subscribe<NativeSettingsPayload>(
        OPEN_SETTINGS_DIALOG_EVENT,
        (payload) => {
          openSettings(payload)
        }
      )
      .then((off) => {
        if (cancelled) off()
        else dispose = off
      })
      .catch((err) => {
        console.warn("[SettingsDialog] subscription failed:", err)
      })

    return () => {
      cancelled = true
      dispose?.()
    }
  }, [openSettings])

  const activePath = useMemo(
    () => settingsSectionToNavPath(state.section),
    [state.section]
  )

  const handleNavigate = useCallback((href: string) => {
    setState((prev) => ({
      section: settingsPathToSection(href),
      agentType: prev.agentType,
    }))
  }, [])

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent
        className="h-[min(760px,calc(100dvh-2rem))] max-w-[min(1120px,calc(100vw-2rem))] gap-0 overflow-hidden rounded-xl p-0"
        closeButtonClassName="top-1.5 right-2 z-50 h-7 w-7"
      >
        <DialogTitle className="sr-only">{t("title")}</DialogTitle>
        <SettingsShell
          activePath={activePath}
          className="h-full rounded-xl"
          onBack={() => setOpen(false)}
          onNavigate={handleNavigate}
          showToaster={false}
          showWindowControls={false}
          updateDocumentTitle={false}
        >
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
                {tPages("agentsLoading")}
              </div>
            }
          >
            <SettingsDialogBody {...state} />
          </Suspense>
        </SettingsShell>
      </DialogContent>
    </Dialog>
  )
}
