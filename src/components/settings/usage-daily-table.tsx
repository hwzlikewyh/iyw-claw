"use client"

import { Fragment, useState } from "react"
import { ChevronDown } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { formatTokenCount } from "@/lib/token-format"
import type { UsageDailyRow } from "@/lib/usage-stats"
import { cn } from "@/lib/utils"
import { formatUsagePoints } from "./usage-presentation"

function DayDetails({ row }: { row: UsageDailyRow }) {
  const t = useTranslations("UsageSettings")
  const locale = useLocale()
  const fields = [
    "input",
    "output",
    "cacheRead",
    "cacheWrite",
    "sessions",
  ] as const
  return (
    <dl className="grid grid-cols-2 gap-x-5 gap-y-3 bg-muted/30 p-3 @lg:grid-cols-3">
      {fields.map((key) => (
        <div key={key} className="space-y-1">
          <dt className="text-muted-foreground">{t(`table.${key}`)}</dt>
          <dd className="tabular-nums">
            {key === "sessions"
              ? row[key].toLocaleString(locale)
              : formatTokenCount(row[key])}
          </dd>
        </div>
      ))}
      <div className="space-y-1">
        <dt className="text-muted-foreground">{t("table.cacheHitRate")}</dt>
        <dd className="tabular-nums">{(row.cacheHitRate * 100).toFixed(1)}%</dd>
      </div>
    </dl>
  )
}

function DayExpandButton({
  date,
  expanded,
  onClick,
}: {
  date: string
  expanded: boolean
  onClick: () => void
}) {
  const t = useTranslations("UsageSettings")
  return (
    <button
      type="button"
      aria-expanded={expanded}
      aria-controls={`usage-day-${date}`}
      aria-label={t("daily.details", { date })}
      onClick={onClick}
      className="inline-flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    >
      <ChevronDown
        className={cn("size-4 transition-transform", expanded && "rotate-180")}
      />
    </button>
  )
}

function DayRow({ row }: { row: UsageDailyRow }) {
  const locale = useLocale()
  const [expanded, setExpanded] = useState(false)
  const detailsId = `usage-day-${row.date}`
  return (
    <Fragment>
      <tr className="border-b border-border/60 hover:bg-muted/25">
        <td className="py-3 pe-2 font-medium tabular-nums">
          {row.date.slice(5)}
        </td>
        <td className="px-2 py-3 text-right font-medium tabular-nums">
          {formatUsagePoints(row.totalPoints, locale)}
        </td>
        <td
          className="px-2 py-3 text-right tabular-nums"
          title={row.total.toLocaleString(locale)}
        >
          {formatTokenCount(row.total)}
        </td>
        {(["input", "output", "cacheRead"] as const).map((key) => (
          <td
            key={key}
            className="hidden px-2 py-3 text-right tabular-nums text-muted-foreground @2xl:table-cell"
            title={row[key].toLocaleString(locale)}
          >
            {formatTokenCount(row[key])}
          </td>
        ))}
        <td className="hidden px-2 py-3 text-right tabular-nums text-muted-foreground @lg:table-cell">
          {row.sessions.toLocaleString(locale)}
        </td>
        <td className="w-8 py-1 text-right">
          <DayExpandButton
            date={row.date}
            expanded={expanded}
            onClick={() => setExpanded(!expanded)}
          />
        </td>
      </tr>
      {expanded && (
        <tr id={detailsId}>
          <td colSpan={8} className="pb-2">
            <DayDetails row={row} />
          </td>
        </tr>
      )}
    </Fragment>
  )
}

export function UsageDailyTable({ rows }: { rows: UsageDailyRow[] }) {
  const t = useTranslations("UsageSettings")
  return (
    <section className="min-w-0 space-y-3">
      <h2 className="text-sm font-semibold">{t("daily.records")}</h2>
      <table className="w-full text-xs">
        <thead className="border-b text-muted-foreground">
          <tr>
            <th scope="col" className="py-2 pe-2 text-left font-normal">
              {t("table.date")}
            </th>
            <th scope="col" className="px-2 py-2 text-right font-normal">
              {t("table.points")}
            </th>
            <th scope="col" className="px-2 py-2 text-right font-normal">
              Token
            </th>
            {(["input", "output", "cacheRead"] as const).map((key) => (
              <th
                key={key}
                scope="col"
                className="hidden px-2 py-2 text-right font-normal @2xl:table-cell"
              >
                {t(`table.${key}`)}
              </th>
            ))}
            <th
              scope="col"
              className="hidden px-2 py-2 text-right font-normal @lg:table-cell"
            >
              {t("table.sessions")}
            </th>
            <th scope="col" className="w-8">
              <span className="sr-only">{t("daily.records")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {[...rows].reverse().map((row) => (
            <DayRow key={row.date} row={row} />
          ))}
        </tbody>
      </table>
    </section>
  )
}
