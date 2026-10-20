"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import { CalendarClock, CircleCheck, ExternalLink, Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import type { ToolCallState } from "@/lib/adapters/ai-elements-adapter"

interface ScheduledTaskReceipt {
  id: number
  name: string
  enabled: boolean
  cron: string
  timezone: string
  nextRunAt: string | null
}

interface Props {
  output?: string | null
  state?: ToolCallState
}

export function ScheduledTaskResultCard({ output, state }: Props) {
  const t = useTranslations("Folder.chat.scheduledTaskResult")
  const receipt = useMemo(() => parseScheduledTaskReceipt(output), [output])
  const running = state === "input-available" || state === "input-streaming"
  if (!receipt && !running) return null
  if (!receipt) return <CreatingCard label={t("creating")} />
  return <VerifiedCard receipt={receipt} />
}

function CreatingCard({ label }: { label: string }) {
  return (
    <div className="mb-2 flex items-center gap-2 rounded-xl border bg-card p-3 text-sm">
      <Loader2 className="size-4 animate-spin text-muted-foreground" />
      <span>{label}</span>
    </div>
  )
}

function VerifiedCard({ receipt }: { receipt: ScheduledTaskReceipt }) {
  const t = useTranslations("Folder.chat.scheduledTaskResult")
  const { setRoute } = useWorkbenchRoute()
  const fields = [
    [t("taskId"), String(receipt.id)],
    [t("schedule"), receipt.cron],
    [t("timezone"), receipt.timezone],
    [t("nextRun"), formatNextRun(receipt.nextRunAt, t("pendingNextRun"))],
    [t("status"), receipt.enabled ? t("enabled") : t("disabled")],
  ]
  return (
    <div className="mb-2 overflow-hidden rounded-xl border border-emerald-500/30 bg-card">
      <div className="flex items-start gap-3 p-3">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
          <CircleCheck className="size-5" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium">{t("verifiedTitle")}</p>
          <p className="truncate text-sm text-foreground">{receipt.name}</p>
          <dl className="mt-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
            {fields.map(([label, value]) => (
              <Field key={label} label={label} value={value} />
            ))}
          </dl>
        </div>
      </div>
      <div className="flex justify-end border-t bg-muted/20 px-3 py-2">
        <Button type="button" variant="ghost" size="xs" onClick={() => setRoute("automations")}>
          <CalendarClock data-icon="inline-start" />
          {t("openTasks")}
          <ExternalLink data-icon="inline-end" />
        </Button>
      </div>
    </div>
  )
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-muted-foreground">{label}</dt>
      <dd>{value}</dd>
    </>
  )
}

function formatNextRun(value: string | null, fallback: string): string {
  if (!value) return fallback
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}

export function parseScheduledTaskReceipt(output: string | null | undefined) {
  const receipt = findReceipt(parseJson(output), 5)
  if (!receipt) return null
  const task = asRecord(receipt.task)
  const id = integer(task?.id)
  const name = text(task?.name)
  const cron = text(task?.cron)
  const timezone = text(task?.timezone)
  if (!id || !name || !cron || !timezone) return null
  return {
    id, name, cron, timezone,
    enabled: task?.enabled === true,
    nextRunAt: text(task?.next_run_at),
  }
}

function findReceipt(value: unknown, depth: number): Record<string, unknown> | null {
  if (depth <= 0) return null
  const record = asRecord(value)
  if (!record) {
    return typeof value === "string" ? findReceipt(parseJson(value), depth - 1) : null
  }
  if (record.status === "verified" && record.verified === true && asRecord(record.task)) {
    return record
  }
  for (const key of ["structuredContent", "structured_content", "result", "output", "Ok", "ok"]) {
    const found = findReceipt(record[key], depth - 1)
    if (found) return found
  }
  if (Array.isArray(record.content)) {
    for (const item of record.content) {
      const found = findReceipt(asRecord(item)?.text, depth - 1)
      if (found) return found
    }
  }
  return null
}

function parseJson(value: unknown): unknown {
  if (typeof value !== "string" || !value.trim()) return value
  const body = value.split(/Output:\r?\n/).at(-1)?.trim() ?? value.trim()
  try { return JSON.parse(body) } catch { return null }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>) : null
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null
}

function integer(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null
}
