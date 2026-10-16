"use client"

import { useTranslations } from "next-intl"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { Automation } from "@/lib/types"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"

export interface RunFiltersValue {
  folder: string
  status: string
  trigger: string
  task: string
  search: string
}

export function AutomationRunFilters({
  value,
  onChange,
  automations,
  single,
}: {
  value: RunFiltersValue
  onChange: (value: RunFiltersValue) => void
  automations: Automation[]
  single: boolean
}) {
  const t = useTranslations("Automations")
  const folders = useAppWorkspaceStore((store) => store.folders)
  const select = (
    key: "status" | "trigger" | "task" | "folder",
    label: string,
    choices: Array<[string, string]>
  ) => (
    <Select
      value={value[key]}
      onValueChange={(next) => onChange({ ...value, [key]: next })}
    >
      <SelectTrigger size="sm" className="max-w-48 text-xs" aria-label={label}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {choices.map(([id, text]) => (
          <SelectItem key={id} value={id}>
            {text}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
  return (
    <div className="flex flex-wrap items-center gap-2">
      {!single && (
        <input
          type="search"
          className="h-8 w-44 min-w-0 rounded-md border border-border bg-background px-3 text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring"
          aria-label={t("searchRuns")}
          placeholder={t("searchRuns")}
          value={value.search}
          onChange={(event) =>
            onChange({ ...value, search: event.target.value })
          }
        />
      )}
      {select("status", t("allStatuses"), [
        ["all", t("allStatuses")],
        ["running", t("statusRunning")],
        ["succeeded", t("statusSucceeded")],
        ["failed", t("statusFailed")],
        ["cancelled", t("statusCancelled")],
        ["skipped", t("statusSkipped")],
      ])}
      {select("trigger", t("allTriggers"), [
        ["all", t("allTriggers")],
        ["schedule", t("triggerSchedule")],
        ["manual", t("manual")],
        ["retry", t("retryRun")],
      ])}
      {!single &&
        select("folder", t("allFolders"), [
          ["all", t("allFolders")],
          ...folders.map((folder): [string, string] => [
            String(folder.id),
            folder.name,
          ]),
        ])}
      {!single &&
        select("task", t("allTasks"), [
          ["all", t("allTasks")],
          ...automations.map((task): [string, string] => [
            String(task.id),
            task.name,
          ]),
        ])}
    </div>
  )
}
