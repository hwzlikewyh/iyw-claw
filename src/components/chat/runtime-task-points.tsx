"use client"

import { useTranslations } from "next-intl"
import { useState } from "react"
import { UsagePointsValue } from "@/components/message/usage-points"
import type { RuntimeConsumption } from "@/hooks/use-runtime-task-consumption"
import type { ConfirmedConsumption } from "@/lib/types"
import type { RuntimeTask } from "@/lib/runtime-tasks"

export function SessionTaskPoints({
  consumption,
}: {
  consumption: RuntimeConsumption | null
}) {
  const t = useTranslations("RuntimeTasks")
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 border-t px-1 py-2 text-muted-foreground">
      <span>
        {t("sessionPoints")}:{" "}
        <UsagePointsValue
          points={consumption?.session.amount}
          consumption={consumption?.session}
        />
      </span>
      {consumption?.session && <PointDetails points={consumption.session} />}
      <span>
        {t("ownPoints")}:{" "}
        <UsagePointsValue
          points={consumption?.own?.amount}
          consumption={consumption?.own}
        />
      </span>
    </div>
  )
}

export function TaskPoints({
  task,
  consumption,
}: {
  task: RuntimeTask
  consumption: RuntimeConsumption | null
}) {
  const t = useTranslations("RuntimeTasks")
  const key = task.kind === "goal" ? `goal:${task.native_id}` : task.id
  const points =
    consumption?.scopes?.[key] ??
    (task.tool_call_id
      ? consumption?.scopes?.[`subagent:${task.tool_call_id}`]
      : undefined)
  return (
    <div className="rounded-lg bg-muted/40 p-2 text-xs">
      <div className="flex flex-wrap justify-between gap-2">
        <span>{t("taskPoints")}</span>
        <UsagePointsValue points={points?.amount} consumption={points} />
      </div>
      {points ? (
        <PointDetails points={points} />
      ) : (
        <p className="mt-1 text-muted-foreground">{t("scopePending")}</p>
      )}
    </div>
  )
}

function PointDetails({ points }: { points: ConfirmedConsumption }) {
  const t = useTranslations("RuntimeTasks")
  const statuses = Object.entries(points.settlement_statuses ?? {})
  const settled =
    statuses.length > 0 &&
    statuses.every(([status]) =>
      ["charged", "settled_without_debit", "revoked"].includes(status)
    )
  return (
    <details className="mt-1 text-muted-foreground">
      <summary className="cursor-pointer">{t("pointDetails")}</summary>
      <div className="mt-1 space-y-1 break-all">
        <p>
          {t("exactPoints")}:{" "}
          <span className="tabular-nums">
            {points.amount ?? t("pointUnknown")}
          </span>
        </p>
        <p>
          {t("ledgerPoints")}:{" "}
          <span className="tabular-nums">
            {points.charged_points ?? t("pointUnknown")}
          </span>
        </p>
        <p>{t(settled ? "settlementConfirmed" : "settlementPending")}</p>
        <p>
          {t(
            points.state === "confirmed"
              ? "pointConfirmed"
              : points.state === "partial"
                ? "pointPartial"
                : "pointUnavailable"
          )}
        </p>
      </div>
      <PointRecords points={points} />
    </details>
  )
}

function PointRecords({ points }: { points: ConfirmedConsumption }) {
  const t = useTranslations("RuntimeTasks")
  const [open, setOpen] = useState(false)
  if (!points.records?.length) return null
  return (
    <details
      onToggle={(event) => setOpen(event.currentTarget.open)}
      className="mt-2"
    >
      <summary className="cursor-pointer">
        {t("recordCount", { count: points.records.length })}
      </summary>
      {open && (
        <div className="mt-1 max-h-48 space-y-2 overflow-auto">
          {points.records.map((row) => (
            <div
              key={JSON.stringify([row.request_id, row.stage_key])}
              className="break-all rounded border p-2 font-mono text-xs"
            >
              <p>
                {row.request_id} / {row.stage_key}
              </p>
              <p>
                {t("exactPoints")}: {row.actual_points ?? t("pointUnknown")}
              </p>
              <p>
                {t("ledgerPoints")}: {row.charged_points ?? t("pointUnknown")}
              </p>
              <p>{row.settlement_status || t("pointUnknown")}</p>
            </div>
          ))}
        </div>
      )}
    </details>
  )
}
