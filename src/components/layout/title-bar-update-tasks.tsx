"use client"

import { Clock3, Loader2, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"

import type { UpdateContextValue } from "@/components/providers/update-provider"
import { Button } from "@/components/ui/button"
import { normalizeAppUpdateError, usesTauriUpdater } from "@/lib/updater"

const BLOCKER_KEYS = {
  agent_operation_active: "updateTasks.agentActive",
  automation_active: "updateTasks.automationActive",
  voice_active: "updateTasks.voiceActive",
  terminal_open: "updateTasks.terminalOpen",
  interaction_pending: "updateTasks.interactionPending",
  unrecoverable_task: "updateTasks.unrecoverable",
  pause_timeout: "updateTasks.pauseTimeout",
  recovery_pending: "updateTasks.recoveryPending",
} as const

export function UpdateTaskStatus({ update }: { update: UpdateContextValue }) {
  const t = useTranslations("SystemSettings")
  const { state } = update
  if (state.status === "preparing_restart") {
    return (
      <p
        role="status"
        className="flex items-center gap-2 text-sm text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" />
        {t("updateTasks.saving")}
      </p>
    )
  }
  const waiting = state.status === "waiting_for_tasks"
  const reason = waiting
    ? state.blockedReason
    : state.error?.split("update_blocked:")[1]?.trim()
  const key =
    reason && reason in BLOCKER_KEYS
      ? BLOCKER_KEYS[reason as keyof typeof BLOCKER_KEYS]
      : undefined
  return (
    <div role="status" className="space-y-1 text-sm text-muted-foreground">
      <p>
        {waiting
          ? t("updateTasks.waiting")
          : t(key ?? "updateErrors.tasksActive")}
      </p>
      {waiting && key ? <p>{t(key)}</p> : null}
    </div>
  )
}

export function hasUpdateTaskStatus(update: UpdateContextValue) {
  const { state } = update
  return (
    state.status === "waiting_for_tasks" ||
    state.status === "preparing_restart" ||
    (state.status === "error" &&
      normalizeAppUpdateError(state.error).kind === "tasks_active")
  )
}

export function UpdateTaskActions({ update }: { update: UpdateContextValue }) {
  const t = useTranslations("SystemSettings")
  if (!usesTauriUpdater()) return null
  if (update.state.status === "waiting_for_tasks") {
    return (
      <Button
        variant="outline"
        onClick={() => void update.startUpdate("cancel_waiting")}
      >
        {t("updateTasks.cancel")}
      </Button>
    )
  }
  const available = ["ready_to_install", "available", "error"].includes(
    update.state.status
  )
  if (!update.state.version || !available || update.isBusy) return null
  return (
    <div className="w-full space-y-2">
      <div className="flex flex-wrap justify-end gap-2">
        <Button
          variant="outline"
          onClick={() => void update.startUpdate("after_tasks")}
        >
          <Clock3 />
          {t("updateTasks.afterTasks")}
        </Button>
        <Button
          variant="outline"
          onClick={() => void update.startUpdate("force_resume")}
        >
          <RotateCcw />
          {t("updateTasks.force")}
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">
        {t("updateTasks.forceHint")}
      </p>
    </div>
  )
}

export function UpdateRecoveryNotice({
  update,
}: {
  update: UpdateContextValue
}) {
  const t = useTranslations("SystemSettings")
  if (!update.state.recoveryError || !usesTauriUpdater()) return null
  return (
    <div role="alert" className="space-y-2 text-sm text-destructive">
      <p>{t("updateTasks.recoveryFailed")}</p>
      <details>
        <summary>{t("updateTasks.details")}</summary>
        <p className="break-words">{update.state.recoveryError}</p>
      </details>
      <Button
        variant="outline"
        disabled={update.isBusy}
        onClick={() => void update.startUpdate("retry_recovery")}
      >
        {t("updateTasks.retryRecovery")}
      </Button>
    </div>
  )
}
