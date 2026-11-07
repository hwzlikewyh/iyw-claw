"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  connectorTaskActive,
  type ConnectorInstallTask,
} from "@/lib/connector-install-tasks"
import type { McpMarketplaceItem } from "@/lib/types"

export function ConnectorInstallTasks({
  tasks,
  items,
  onOpen,
}: {
  tasks: ConnectorInstallTask[]
  items: McpMarketplaceItem[]
  onOpen: (item: McpMarketplaceItem) => void
}) {
  const t = useTranslations("ConnectorMarket")
  const actionable = latestTasks(tasks).filter(
    (task) =>
      connectorTaskActive(task) ||
      task.phase === "failed" ||
      task.phase === "interrupted"
  )
  if (!actionable.length) return null
  return (
    <section className="space-y-2" aria-label={t("installationTasks")}>
      <h3 className="text-sm font-medium">{t("installationTasks")}</h3>
      {actionable.map((task) => {
        const item =
          items.find((item) => item.server_id === task.serverId) ||
          itemForTask(task)
        return <TaskRow key={task.id} task={task} item={item} onOpen={onOpen} />
      })}
    </section>
  )
}

function TaskRow({
  task,
  item,
  onOpen,
}: {
  task: ConnectorInstallTask
  item: McpMarketplaceItem
  onOpen: (item: McpMarketplaceItem) => void
}) {
  const t = useTranslations("ConnectorMarket")
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border p-3 text-sm">
      <div className="min-w-0">
        <p className="break-words">
          {item.name} · {t(`task_${task.phase}`)}
        </p>
        {task.error ? (
          <p className="mt-1 break-words text-xs text-destructive">
            {task.error}
          </p>
        ) : null}
      </div>
      <Button variant="outline" size="sm" onClick={() => onOpen(item)}>
        {t("viewInstallation")}
      </Button>
    </div>
  )
}

function latestTasks(tasks: ConnectorInstallTask[]) {
  const seen = new Set<string>()
  return tasks.filter((task) => {
    if (seen.has(task.serverId)) return false
    seen.add(task.serverId)
    return true
  })
}

function itemForTask(task: ConnectorInstallTask): McpMarketplaceItem {
  return {
    provider_id: "iyw_gateway",
    server_id: task.serverId,
    name: task.serverId,
    description: "",
    homepage: null,
    remote: false,
    verified: false,
    icon_url: null,
    latest_version: null,
    protocols: [],
    owner: null,
    namespace: null,
    downloads: null,
    score: null,
    is_deployed: null,
  }
}
