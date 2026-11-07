"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  connectorTaskActive,
  type ConnectorInstallTask,
} from "@/lib/connector-install-tasks"

export function ConnectorInstallTaskStatus({
  task,
  onCancel,
}: {
  task: ConnectorInstallTask
  onCancel: () => void
}) {
  const t = useTranslations("ConnectorMarket")
  const active = connectorTaskActive(task)
  return (
    <section className="space-y-2 rounded-md border p-3" aria-live="polite">
      <p className="text-sm font-medium">{t(`task_${task.phase}`)}</p>
      {task.phase === "downloading" ? (
        <>
          <progress
            className="h-2 w-full accent-primary"
            value={task.totalBytes ? task.receivedBytes : undefined}
            max={task.totalBytes || undefined}
            aria-label={t("task_downloading")}
          />
          <p className="text-xs text-muted-foreground">
            {(task.receivedBytes / 1024 / 1024).toFixed(1)} MiB
            {task.totalBytes
              ? ` / ${(task.totalBytes / 1024 / 1024).toFixed(1)} MiB`
              : ""}
          </p>
        </>
      ) : null}
      {task.error ? (
        <p className="break-words text-xs text-destructive">{task.error}</p>
      ) : null}
      {active ? (
        <p className="text-xs text-muted-foreground">{t("taskBackground")}</p>
      ) : null}
      {active && task.cancellable ? (
        <Button
          size="sm"
          variant="outline"
          disabled={task.phase === "cancelling"}
          onClick={onCancel}
        >
          {t("cancelInstallation")}
        </Button>
      ) : null}
    </section>
  )
}
