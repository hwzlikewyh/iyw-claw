"use client"

import {
  ArrowRight,
  ChevronRight,
  Clock,
  Folder,
  MessageCircle,
  Pencil,
  Play,
} from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { AgentIcon } from "@/components/agent-icon"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import type { Automation, ChatChannelInfo } from "@/lib/types"
import { ScheduleLabel } from "./schedule-label"
import { AutomationStatus, dateTime } from "./automation-presentation"
import styles from "./automation-workspace.module.css"

export function AutomationTaskCard({
  automation: task,
  folder,
  channels,
  busy,
  onSelect,
  onRun,
  onEdit,
  onToggle,
  onNotify,
}: {
  automation: Automation
  folder: string
  channels: ChatChannelInfo[]
  busy: boolean
  onSelect: () => void
  onRun: () => void
  onEdit: () => void
  onToggle: () => void
  onNotify: () => void
}) {
  const t = useTranslations("Automations")
  const locale = useLocale()
  const channelId = task.config?.notification_channel_id
  const channel = channels.find((item) => item.id === channelId)
  return (
    <article
      className={styles.card}
      data-paused={!task.enabled}
      tabIndex={0}
      aria-label={task.name}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (
          event.target === event.currentTarget &&
          ["Enter", " "].includes(event.key)
        ) {
          event.preventDefault()
          onSelect()
        }
      }}
    >
      <div className={styles.cardTop}>
        <span className={styles.agentIcon}>
          <AgentIcon agentType={task.agent_type} className="size-5" />
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-[15px] font-semibold">{task.name}</h2>
          <p className="mt-0.5 text-[11px] text-muted-foreground">
            {task.config?.label_snapshot?.agent_label ||
              getAgentDisplayName(task.agent_type)}
          </p>
        </div>
        <div onClick={(event) => event.stopPropagation()}>
          <Switch
            aria-label={task.enabled ? t("disable") : t("enable")}
            checked={task.enabled}
            disabled={busy}
            onCheckedChange={onToggle}
          />
        </div>
      </div>
      <p className={styles.description}>{task.config?.display_text || "—"}</p>
      <div className="mt-3 flex min-w-0 items-center gap-1.5 text-[10px] text-muted-foreground">
        <Folder className="size-3 shrink-0" />
        <span className="truncate">{folder}</span>
      </div>
      <div className={styles.schedule}>
        <span className="flex min-w-0 items-center gap-2 text-[13px] font-medium">
          <Clock className="size-3.5 shrink-0 opacity-60" />
          {task.cron ? (
            <ScheduleLabel cron={task.cron} emphasizeTime />
          ) : (
            t("manual")
          )}
        </span>
        <div className="shrink-0 text-right">
          <span className="block text-[9px] text-muted-foreground">
            {t("nextRun")}
          </span>
          <span className="text-[10px]">
            {task.enabled
              ? dateTime(task.next_run_at, locale, task.timezone)
              : t("statusDisabled")}
          </span>
        </div>
      </div>
      <button
        type="button"
        className={styles.notification}
        onClick={(event) => {
          event.stopPropagation()
          onNotify()
        }}
      >
        <MessageCircle className="size-3.5 shrink-0" />
        <span className="shrink-0">{t("notificationState")}</span>
        <span className="ml-auto truncate">
          {channelId
            ? channel?.name || t("channelUnavailable")
            : t("notificationChoose")}
        </span>
        <ChevronRight className="size-3 shrink-0" />
      </button>
      <div className={styles.cardFooter}>
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <AutomationStatus status={task.last_run_status} />
          <span className="text-[9px] text-muted-foreground">
            {task.last_run_at ? dateTime(task.last_run_at, locale) : ""}
          </span>
        </div>
        <div
          className="ml-auto flex shrink-0 items-center gap-0.5"
          onClick={(event) => event.stopPropagation()}
        >
          <Button
            variant="ghost"
            size="icon-sm"
            title={t("runNow")}
            aria-label={t("runNow")}
            disabled={busy || task.last_run_status === "running"}
            onClick={onRun}
          >
            <Play className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            title={t("edit")}
            aria-label={t("edit")}
            onClick={onEdit}
          >
            <Pencil className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="px-1.5 text-[11px] text-primary"
            onClick={onSelect}
          >
            {t("records")}
            <ArrowRight className="size-3" />
          </Button>
        </div>
      </div>
    </article>
  )
}
