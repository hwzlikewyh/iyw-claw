"use client"
import type { ReactNode } from "react"
import { useLocale, useTranslations } from "next-intl"
import type { Automation, AutomationRun } from "@/lib/types"
import { AutomationRunNotification } from "./automation-run-actions"
import { useAutomationErrorMessage } from "./automation-run-error"
import {
  AutomationStatus,
  dateTime,
  runDuration,
} from "./automation-presentation"
import styles from "./automation-workspace.module.css"

interface HistoryRowsProps {
  runs: AutomationRun[]
  automation?: Automation
  taskFor: (run: AutomationRun) => Automation | undefined
  onSelectTask: (task: Automation) => void
  actions: (run: AutomationRun) => ReactNode
}

export function AutomationHistoryRows({
  runs,
  automation,
  taskFor,
  onSelectTask,
  actions,
}: HistoryRowsProps) {
  const t = useTranslations("Automations")
  const locale = useLocale()
  const errorMessage = useAutomationErrorMessage()
  return automation ? (
    <ol className={styles.timeline}>
      {runs.map((run) => (
        <li key={run.id} className={styles.run}>
          <time className="pt-4 text-xs text-muted-foreground">
            {dateTime(
              run.started_at || run.created_at,
              locale,
              automation.timezone
            )}
          </time>
          <article className={styles.runBody}>
            <div className="flex flex-wrap items-center gap-3">
              <AutomationStatus
                status={run.status}
                retry={run.trigger === "retry"}
              />
              <span className="rounded border border-border px-1.5 text-[10px] text-muted-foreground">
                {t(
                  run.trigger === "retry"
                    ? "retryRun"
                    : run.trigger === "manual"
                      ? "manual"
                      : "triggerSchedule"
                )}
              </span>
              <span className="ml-auto text-[10px] text-muted-foreground">
                {runDuration(run.started_at, run.ended_at)}
              </span>
            </div>
            <p
              className={
                run.error
                  ? "my-3 whitespace-pre-wrap text-xs leading-relaxed text-destructive"
                  : "my-3 line-clamp-4 whitespace-pre-wrap text-xs leading-relaxed text-muted-foreground"
              }
            >
              {run.error
                ? errorMessage(run.error)
                : run.summary || t("runNumber", { id: run.id })}
            </p>
            <AutomationRunNotification run={run} />
            {actions(run)}
          </article>
        </li>
      ))}
    </ol>
  ) : (
    <div className={styles.tableWrap}>
      <table className={styles.table}>
        <thead>
          <tr>
            <th>{t("name")}</th>
            <th>{t("runStatus")}</th>
            <th>{t("runTrigger")}</th>
            <th>{t("runTime")}</th>
            <th>{t("runDuration")}</th>
            <th>{t("notificationState")}</th>
            <th>{t("runActions")}</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((run) => (
            <tr key={run.id}>
              <td>
                <button
                  className="text-left hover:text-primary"
                  onClick={() => {
                    const task = taskFor(run)
                    if (task) onSelectTask(task)
                  }}
                >
                  {taskFor(run)?.name || t("taskRemoved")}
                </button>
                {run.error && (
                  <p className="mt-1 max-w-sm whitespace-pre-wrap text-xs text-destructive">
                    {errorMessage(run.error)}
                  </p>
                )}
              </td>
              <td>
                <AutomationStatus
                  status={run.status}
                  retry={run.trigger === "retry"}
                />
              </td>
              <td className="text-muted-foreground">
                {t(
                  run.trigger === "retry"
                    ? "retryRun"
                    : run.trigger === "manual"
                      ? "manual"
                      : "triggerSchedule"
                )}
              </td>
              <td className="text-muted-foreground">
                {dateTime(run.started_at || run.created_at, locale)}
              </td>
              <td className="text-muted-foreground">
                {runDuration(run.started_at, run.ended_at)}
              </td>
              <td>
                <AutomationRunNotification run={run} />
              </td>
              <td>{actions(run)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
