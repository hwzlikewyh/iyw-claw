"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { DownloadProgress } from "./download-progress"
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
    <section className="space-y-2 rounded-md border p-3">
      {task.phase === "downloading" ? (
        <DownloadProgress
          label={t("task_downloading")}
          receivedBytes={task.receivedBytes}
          totalBytes={task.totalBytes}
        />
      ) : (
        <p className="text-sm font-medium" role="status">
          {t(`task_${task.phase}`)}
        </p>
      )}
      {task.error ? (
        <p className="break-words text-xs text-destructive" role="alert">
          {task.error}
        </p>
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
