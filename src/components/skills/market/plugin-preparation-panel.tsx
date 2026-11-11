"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useTabStore } from "@/contexts/tab-context"
import { onTransportChange } from "@/lib/transport"
import { notifyPluginPreparation } from "@/lib/plugin-preparation-events"
import type { PluginPreparation } from "@/lib/plugin-preparation"
import { usePluginPreparation } from "./use-plugin-preparation"
import { usePluginWorkspace } from "./use-plugin-workspace"
import { PluginPermissionReview } from "./plugin-preparation-details"
import { PreparationActions } from "./plugin-preparation-actions"
import { PreparationSummary } from "./plugin-preparation-summary"
import { PluginConnectorPreparationList } from "./plugin-connector-preparation"
import { PluginRecoveryDialog } from "./plugin-recovery-dialog"
import { PluginVersionsDialog } from "./plugin-versions-dialog"
import { Button } from "@/components/ui/button"
import { PluginApplicationActions } from "./plugin-application-actions"

interface Props {
  marketSkillId: string
  installedVersion: string
  onRepair: () => void
  onOpenConnectors: () => void
  onVersionChanged: () => void
}
interface ContextProps extends Props {
  tabId: string | null
  conversationId: number | null
}

export function PluginPreparationPanel(props: Props) {
  const { profile, status } = useIywAccount()
  const tabId = useTabStore((state) => state.activeTabId)
  const conversationId = useTabStore(
    (state) =>
      state.tabs.find((tab) => tab.id === state.activeTabId)?.conversationId ??
      null
  )
  const [scope, setScope] = useState(0)
  useEffect(() => onTransportChange(() => setScope((value) => value + 1)), [])
  const key = JSON.stringify([
    props.marketSkillId,
    props.installedVersion,
    status,
    profile?.user_id,
    profile?.org_code,
    profile?.workspace_pending,
    tabId,
    conversationId,
    scope,
  ])
  return (
    <PreparationContent
      key={key}
      {...props}
      tabId={tabId}
      conversationId={conversationId}
    />
  )
}

function PreparationContent(props: ContextProps) {
  const t = useTranslations("PluginPreparation")
  const state = usePluginPreparation(props.marketSkillId, props.conversationId)
  const context = usePluginWorkspace(props.tabId)
  const [recovering, setRecovering] = useState(false)
  const [versionsOpen, setVersionsOpen] = useState(false)
  const versionText = useTranslations("PluginVersions")
  const [review, setReview] = useState<{
    view: PluginPreparation
    enabled: boolean
  } | null>(null)
  return (
    <section className="space-y-3 border-b pb-4">
      <h3 className="text-sm font-semibold">{t("title")}</h3>
      <PreparationSummary state={state} contextError={context.error} />
      <PluginApplicationActions
        pluginId={props.marketSkillId}
        conversationId={props.conversationId}
      />
      <PluginConnectorPreparationList
        id={props.marketSkillId}
        conversationId={props.conversationId}
        state={state}
      />
      <PreparationActions
        {...props}
        onRepair={() => setRecovering(true)}
        state={state}
        contextBusy={context.busy}
        onBind={() => void context.bind()}
        onReview={(enabled) => {
          if (!state.view) return
          if (enabled) void state.mutate(state.view, true)
          else setReview({ view: state.view, enabled })
        }}
      />
      <Button
        size="sm"
        variant="outline"
        disabled={state.busy || context.busy}
        onClick={() => setVersionsOpen(true)}
      >
        {versionText("open")}
      </Button>
      {versionsOpen ? (
        <PluginVersionsDialog
          id={props.marketSkillId}
          onClose={() => setVersionsOpen(false)}
          onChanged={() => {
            state.refresh()
            props.onVersionChanged()
          }}
          onReinstall={props.onRepair}
        />
      ) : null}
      {recovering ? (
        <PluginRecoveryDialog
          id={props.marketSkillId}
          onClose={() => setRecovering(false)}
          onRecovered={state.refresh}
          onReinstall={props.onRepair}
        />
      ) : null}
      {review ? (
        <PluginPermissionReview
          view={review.view}
          enabled={review.enabled}
          busy={state.busy}
          error={state.error}
          onClose={() => {
            setReview(null)
            notifyPluginPreparation({
              pluginId: props.marketSkillId,
              conversationId: props.conversationId,
              canceled: true,
            })
          }}
          onConfirm={() =>
            void state.mutate(review.view, review.enabled).then((success) => {
              if (success) setReview(null)
            })
          }
        />
      ) : null}
    </section>
  )
}
