"use client"

import { ArrowLeft, Clock, Folder, Pencil, Play, Trash2 } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { AgentIcon } from "@/components/agent-icon"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type { Automation } from "@/lib/types"
import { ScheduleLabel } from "./schedule-label"
import { dateTime } from "./automation-presentation"
import { AutomationHistory } from "./automation-history"
import styles from "./automation-workspace.module.css"

export function AutomationTaskDetail({
  task,
  automations,
  folder,
  busy,
  onBack,
  onEdit,
  onRun,
  onToggle,
  onNotify,
  onDelete,
  onChanged,
}: {
  task: Automation
  automations: Automation[]
  folder: string
  busy: boolean
  onBack: () => void
  onEdit: () => void
  onRun: () => void
  onToggle: () => void
  onNotify: () => void
  onDelete: () => void
  onChanged: () => Promise<void>
}) {
  const t = useTranslations("Automations")
  const locale = useLocale()
  return (
    <div className={styles.detail}>
      <Button
        variant="ghost"
        size="sm"
        className="mb-5 -ml-2 text-muted-foreground"
        onClick={onBack}
      >
        <ArrowLeft className="size-3.5" />
        {t("returnTasks")}
      </Button>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3">
          <AgentIcon agentType={task.agent_type} className="size-7" />
          <h1 className="truncate text-xl font-semibold">{task.name}</h1>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Switch
            checked={task.enabled}
            disabled={busy}
            aria-label={t("enabled")}
            onCheckedChange={onToggle}
          />
          <Button variant="outline" size="sm" onClick={onEdit}>
            <Pencil className="size-3.5" />
            {t("edit")}
          </Button>
          <Button
            size="sm"
            disabled={busy || task.last_run_status === "running"}
            onClick={onRun}
          >
            <Play className="size-3.5" />
            {t("runNow")}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("delete")}
            onClick={onDelete}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>
      <div className={styles.summary}>
        <p className="whitespace-pre-wrap text-xs leading-loose text-muted-foreground">
          {task.config?.display_text || "—"}
        </p>
        <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3 text-[11px] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5">
            <Clock className="size-3" />
            {task.cron ? <ScheduleLabel cron={task.cron} /> : t("manual")}
          </span>
          <span>{task.timezone}</span>
          <span className="inline-flex items-center gap-1.5">
            <Folder className="size-3" />
            {folder}
          </span>
          <span>{getAgentDisplayName(task.agent_type)}</span>
          <span className="ml-auto">
            {t("nextRun")} ·{" "}
            {task.enabled
              ? dateTime(task.next_run_at, locale, task.timezone)
              : t("statusDisabled")}
          </span>
        </div>
        <div className="mt-4 flex flex-wrap gap-2 text-[10px] text-muted-foreground">
          {Object.entries(task.config?.config_values || {}).map(
            ([key, value]) => (
              <span
                className="rounded border border-border px-2 py-0.5"
                key={key}
              >
                {task.config?.label_snapshot?.config_labels?.[key] || value}
              </span>
            )
          )}
        </div>
        <Button
          variant="link"
          className="mt-4 h-auto px-0 text-xs"
          onClick={onNotify}
        >
          {t("notificationTitle")} ·{" "}
          {task.config?.notification_channel_id
            ? t("edit")
            : t("notificationChoose")}
        </Button>
      </div>
      <AutomationHistory
        key={task.id}
        automation={task}
        automations={automations}
        onChanged={onChanged}
        onSelectTask={() => {}}
      />
    </div>
  )
}
