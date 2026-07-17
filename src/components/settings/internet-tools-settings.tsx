"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { ExternalLink, RefreshCw, Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { AgentReachConfigDialog } from "./agent-reach-config-dialog"
import { InternetChannelPanel } from "./internet-channel-panel"
import { InternetToolCard } from "./internet-tool-card"
import { SkillToggleList, type SkillToggleItem } from "./skill-toggle-list"
import {
  mergeAllManagedSkillsEnabled,
  mergeManagedSkillEnabled,
} from "./skill-toggle-list-model"
import { Button } from "@/components/ui/button"
import {
  managedSkillsGetFamilyState,
  managedSkillsReconcileFamily,
  managedSkillsSetGlobalEnabled,
  managedSkillsSetSkillEnabled,
  internetToolInstall,
  internetToolUninstall,
  internetToolsAgentReachDoctor,
  internetToolsConfigureAgentReach,
  internetToolsDetect,
  internetToolsImportBrowser,
  internetToolsInstallChannels,
  internetToolsListSkills,
  internetToolsOpencliDoctor,
  internetToolsReadSkill,
  internetToolsSyncSkills,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { openUrl } from "@/lib/platform"
import type {
  AgentReachChannel,
  AgentReachConfigKey,
  InternetChannelStatus,
  InternetToolId,
  InternetToolInfo,
  InternetToolSkill,
  ManagedSkillFamilyState,
  SupportedBrowser,
} from "@/lib/types"
import { invalidateAgentSkillsCache } from "@/hooks/use-agent-skills"

const OPENCLI_EXTENSION_URL =
  "https://chromewebstore.google.com/detail/opencli/ildkmabpimmkaediidaifkhjpohdnifk"

export function InternetToolsSettings() {
  const t = useTranslations("InternetToolsSettings")
  const [tools, setTools] = useState<InternetToolInfo[]>([])
  const [skills, setSkills] = useState<InternetToolSkill[]>([])
  const [family, setFamily] = useState<ManagedSkillFamilyState | null>(null)
  const [channels, setChannels] = useState<InternetChannelStatus[]>([])
  const [opencliDoctor, setOpencliDoctor] = useState("")
  const [busyTools, setBusyTools] = useState<Set<InternetToolId>>(new Set())
  const [busyConfig, setBusyConfig] = useState(false)
  const [configOpen, setConfigOpen] = useState(false)

  const refresh = useCallback(async () => {
    try {
      const [nextTools, nextSkills, nextFamily] = await Promise.all([
        internetToolsDetect(),
        internetToolsListSkills(),
        managedSkillsGetFamilyState("internet_tools"),
      ])
      setTools(nextTools)
      setSkills(nextSkills)
      setFamily(nextFamily)
    } catch (error) {
      toast.error(t("toasts.loadFailed"), {
        description: toErrorMessage(error),
      })
    }
  }, [t])

  useEffect(() => {
    void refresh()
  }, [refresh])
  const tool = useCallback(
    (id: InternetToolId) => tools.find((item) => item.id === id) ?? null,
    [tools]
  )
  const install = useCallback(
    async (id: InternetToolId) => {
      setBusyTools((current) => new Set(current).add(id))
      try {
        const installedTool = await internetToolInstall(id)
        setTools((current) =>
          current.map((item) => (item.id === id ? installedTool : item))
        )
        await managedSkillsReconcileFamily("internet_tools")
        await refresh()
        toast.success(t("toasts.installSuccess"))
      } catch (error) {
        toast.error(t("toasts.installFailed"), {
          description: toErrorMessage(error),
        })
      } finally {
        setBusyTools((current) => {
          const next = new Set(current)
          next.delete(id)
          return next
        })
      }
    },
    [refresh, t]
  )
  const uninstall = useCallback(
    async (id: InternetToolId) => {
      setBusyTools((current) => new Set(current).add(id))
      try {
        const uninstalledTool = await internetToolUninstall(id)
        setTools((current) =>
          current.map((item) => (item.id === id ? uninstalledTool : item))
        )
        await refresh()
        toast.success(t("toasts.uninstallSuccess"))
      } catch (error) {
        toast.error(t("toasts.uninstallFailed"), {
          description: toErrorMessage(error),
        })
      } finally {
        setBusyTools((current) => {
          const next = new Set(current)
          next.delete(id)
          return next
        })
      }
    },
    [refresh, t]
  )

  const doctorAgentReach = useCallback(async () => {
    try {
      setChannels(await internetToolsAgentReachDoctor())
    } catch (error) {
      toast.error(t("toasts.doctorFailed"), {
        description: toErrorMessage(error),
      })
    }
  }, [t])

  const doctorOpencli = useCallback(async () => {
    try {
      const result = await internetToolsOpencliDoctor()
      setOpencliDoctor(result.message)
    } catch (error) {
      toast.error(t("toasts.doctorFailed"), {
        description: toErrorMessage(error),
      })
    }
  }, [t])

  const syncSkills = useCallback(async () => {
    try {
      const report = await internetToolsSyncSkills()
      await managedSkillsReconcileFamily("internet_tools")
      await refresh()
      toast.success(t("toasts.syncSuccess", { count: report.synced }))
    } catch (error) {
      toast.error(t("toasts.syncFailed"), {
        description: toErrorMessage(error),
      })
    }
  }, [refresh, t])

  const runConfig = useCallback(
    async (action: () => Promise<void>, success: string) => {
      setBusyConfig(true)
      try {
        await action()
        toast.success(success)
        await doctorAgentReach()
      } catch (error) {
        toast.error(t("toasts.configureFailed"), {
          description: toErrorMessage(error),
        })
      } finally {
        setBusyConfig(false)
      }
    },
    [doctorAgentReach, t]
  )

  const toggleSkills = useMemo<SkillToggleItem[]>(
    () =>
      skills.map((skill) => ({
        id: skill.id,
        category: skill.source,
        displayName: skill.id,
        description: skill.source === "agent_reach" ? "Agent Reach" : "OpenCLI",
        ready: skill.installedCentrally,
      })),
    [skills]
  )

  return (
    <div className="flex h-full min-h-0 flex-col p-3 md:p-4">
      <div className="flex flex-col items-start justify-between gap-3 pb-4 sm:flex-row">
        <div className="min-w-0">
          <h2 className="text-base font-semibold">{t("title")}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("description")}
          </p>
        </div>
        <Button size="sm" variant="outline" onClick={() => void refresh()}>
          <RefreshCw className="h-3.5 w-3.5" />
          {t("refresh")}
        </Button>
      </div>

      <div className="grid gap-3 lg:grid-cols-2">
        <InternetToolCard
          name="Agent Reach"
          info={tool("agent_reach")}
          busy={busyTools.has("agent_reach")}
          onInstall={() => void install("agent_reach")}
          onUninstall={() => void uninstall("agent_reach")}
          onDoctor={() => void doctorAgentReach()}
          doctorLabel={t("runDoctor")}
        />
        <InternetToolCard
          name="OpenCLI"
          info={tool("opencli")}
          busy={busyTools.has("opencli")}
          onInstall={() => void install("opencli")}
          onUninstall={() => void uninstall("opencli")}
          onDoctor={() => void doctorOpencli()}
          doctorLabel={t("checkConnection")}
        />
      </div>

      <div className="mt-3 flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => setConfigOpen(true)}>
          <Settings2 className="h-3.5 w-3.5" />
          {t("configure")}
        </Button>
        <Button size="sm" variant="outline" onClick={() => void syncSkills()}>
          <RefreshCw className="h-3.5 w-3.5" />
          {t("syncSkills")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void openUrl(OPENCLI_EXTENSION_URL)}
        >
          <ExternalLink className="h-3.5 w-3.5" />
          {t("browserExtension")}
        </Button>
      </div>

      {opencliDoctor && (
        <pre className="mt-3 max-h-28 overflow-auto whitespace-pre-wrap border bg-muted/30 p-3 text-xs">
          {opencliDoctor}
        </pre>
      )}

      <InternetChannelPanel channels={channels} />

      <section className="mt-4 min-h-0 flex-1">
        <h3 className="mb-2 text-sm font-semibold">{t("skillsTitle")}</h3>
        {family && skills.length > 0 ? (
          <SkillToggleList
            skills={toggleSkills}
            skillStates={family.skills}
            globalEnabled={family.allEnabled}
            setGlobalEnabled={async (enabled) => {
              const report = await managedSkillsSetGlobalEnabled(
                "internet_tools",
                enabled
              )
              setFamily((current) =>
                mergeAllManagedSkillsEnabled(current, report.enabled)
              )
              return report
            }}
            setSkillEnabled={async (skillId, enabled) => {
              const report = await managedSkillsSetSkillEnabled(
                "internet_tools",
                skillId,
                enabled
              )
              setFamily((current) =>
                mergeManagedSkillEnabled(current, skillId, report.enabled)
              )
              return report
            }}
            categoryOrder={{ agent_reach: 0, opencli: 1 }}
            translateCategory={(category) =>
              category === "agent_reach"
                ? t("categories.agentReach")
                : t("categories.opencli")
            }
            loadContent={internetToolsReadSkill}
            onApplied={(agents) => agents.forEach(invalidateAgentSkillsCache)}
            searchPlaceholder={t("searchSkills")}
            notReadyHint={t("installFirst")}
          />
        ) : (
          <div className="flex h-full min-h-28 items-center justify-center border bg-card px-4 text-center text-sm text-muted-foreground">
            {t("skillsEmpty")}
          </div>
        )}
      </section>

      <AgentReachConfigDialog
        open={configOpen}
        busy={busyConfig}
        onOpenChange={setConfigOpen}
        onSave={(key: AgentReachConfigKey, value: string) =>
          runConfig(
            () => internetToolsConfigureAgentReach(key, value),
            t("toasts.configureSuccess")
          )
        }
        onImportBrowser={(browser: SupportedBrowser) =>
          runConfig(
            () => internetToolsImportBrowser(browser),
            t("toasts.browserImported")
          )
        }
        onInstallChannels={(selected: AgentReachChannel[]) =>
          runConfig(async () => {
            setChannels(await internetToolsInstallChannels(selected))
          }, t("toasts.channelsInstalled"))
        }
      />
    </div>
  )
}
