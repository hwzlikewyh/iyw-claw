"use client"

import { Brain, Loader2, RefreshCw, Save } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  SettingsPageHeader,
  SettingsPageLayout,
} from "@/components/settings/settings-ui"
import type { UserMemorySettingsSnapshot } from "@/lib/user-memory-documents"
import { UserMemoryDiagnosticsPanel } from "./user-memory-diagnostics"
import { UserMemoryDocumentEditor } from "./user-memory-document-editor"
import { UserMemoryPolicyPanel } from "./user-memory-policy-panel"
import { useUserMemorySettingsState } from "./use-user-memory-settings"

type MemoryHealth = {
  availabilityDown: boolean
  companionDown: boolean
  activityDown: boolean
  healthy: boolean
  detail: string | null
}

function memoryHealth(settings: UserMemorySettingsSnapshot): MemoryHealth {
  const availabilityDown =
    !!settings.availability && !settings.availability.available
  const companionDown =
    !!settings.companionHealth && settings.companionHealth.status !== "ready"
  const activityDown =
    !!settings.candidateDiagnostic && !settings.candidateDiagnostic.available
  const detail = availabilityDown
    ? settings.availability?.detail
    : companionDown
      ? settings.companionHealth?.detail
      : activityDown
        ? settings.candidateDiagnostic?.detail
        : null
  return {
    availabilityDown,
    companionDown,
    activityDown,
    healthy: !availabilityDown && !companionDown && !activityDown,
    detail: detail ?? null,
  }
}

function LoadingMemorySettings() {
  const t = useTranslations("UserMemorySettings")
  return (
    <div
      aria-busy="true"
      className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground"
    >
      <Loader2 className="h-4 w-4 animate-spin" />
      {t("loading")}
    </div>
  )
}

function UnavailableMemorySettings({ error, reload }: ErrorStateProps) {
  const t = useTranslations("UserMemorySettings")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-4 text-sm">
      {error && <ErrorNotice message={error} centered />}
      <Button size="sm" variant="outline" onClick={reload}>
        <RefreshCw className="h-3.5 w-3.5" />
        {t("reload")}
      </Button>
    </div>
  )
}

type ErrorStateProps = { error: string | null; reload: () => void }

function ErrorNotice({
  message,
  centered = false,
}: {
  message: string
  centered?: boolean
}) {
  return (
    <div
      role="alert"
      className={`rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400 ${
        centered ? "max-w-lg text-center" : ""
      }`}
    >
      {message}
    </div>
  )
}

function MemoryStatusBadges({
  settings,
  health,
  staleRunningSessions,
}: {
  settings: UserMemorySettingsSnapshot
  health: MemoryHealth
  staleRunningSessions: number
}) {
  const t = useTranslations("UserMemorySettings")
  if (health.availabilityDown)
    return <StatusBadge text={t("status.unavailable")} />
  if (health.companionDown) {
    return <StatusBadge text={t("status.companionUnavailable")} />
  }
  if (health.activityDown) {
    return <StatusBadge text={t("status.candidateUnavailable")} />
  }
  if (staleRunningSessions > 0) {
    return (
      <StatusBadge
        text={t("status.newConversationRequired", {
          count: staleRunningSessions,
        })}
      />
    )
  }
  if (!settings.enabled)
    return <Badge variant="outline">{t("status.disabled")}</Badge>
  return <Badge variant="outline">{t("status.active")}</Badge>
}

function StatusBadge({ text }: { text: string }) {
  return (
    <Badge variant="destructive" className="text-[11px]">
      {text}
    </Badge>
  )
}

function HeaderActions({ state, health }: LoadedProps) {
  const t = useTranslations("UserMemorySettings")
  return (
    <div className="flex flex-wrap items-center gap-2">
      <MemoryStatusBadges
        settings={state.settings}
        health={health}
        staleRunningSessions={state.staleRunningSessions}
      />
      <Button
        size="sm"
        variant="outline"
        onClick={state.reload}
        disabled={state.saving}
      >
        <RefreshCw className="h-3.5 w-3.5" />
        {t("reload")}
      </Button>
      <Button
        size="sm"
        onClick={() => void state.save()}
        disabled={!state.dirty || state.saving}
      >
        {state.saving ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <Save className="h-3.5 w-3.5" />
        )}
        {state.saving ? t("saving") : t("save")}
      </Button>
    </div>
  )
}

type LoadedState = ReturnType<typeof useUserMemorySettingsState> & {
  settings: UserMemorySettingsSnapshot
  draft: NonNullable<ReturnType<typeof useUserMemorySettingsState>["draft"]>
}
type LoadedProps = { state: LoadedState; health: MemoryHealth }

function LoadedMemorySettings({ state, health }: LoadedProps) {
  const t = useTranslations("UserMemorySettings")
  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Brain}
        title={t("title")}
        description={t("description")}
        action={<HeaderActions state={state} health={health} />}
      />
      {!health.healthy && (
        <ErrorNotice message={health.detail ?? t("status.unavailable")} />
      )}
      {state.error && <ErrorNotice message={state.error} />}
      <UserMemoryDiagnosticsPanel
        settings={state.settings}
        busy={state.saving}
      />
      <UserMemoryDocumentEditor
        activeDocumentId={state.activeDocumentId}
        settings={state.settings}
        draft={state.draft}
        markerProtected={state.markerProtected}
        dirty={state.dirty}
        saving={state.saving}
        onDocumentChange={state.setActiveDocumentId}
        onDraftChange={state.setDraft}
      />
      <UserMemoryPolicyPanel
        draft={state.draft}
        disabled={state.saving}
        onChange={state.setDraft}
      />
    </SettingsPageLayout>
  )
}

export function UserMemorySettings() {
  const state = useUserMemorySettingsState()
  if (state.loading) return <LoadingMemorySettings />
  if (!state.settings || !state.draft) {
    return (
      <UnavailableMemorySettings error={state.error} reload={state.reload} />
    )
  }
  const loaded = { ...state, settings: state.settings, draft: state.draft }
  return (
    <LoadedMemorySettings
      state={loaded}
      health={memoryHealth(state.settings)}
    />
  )
}
