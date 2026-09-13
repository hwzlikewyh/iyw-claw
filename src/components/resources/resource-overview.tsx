"use client"

import { LibraryBig, PackageCheck } from "lucide-react"
import { useTranslations } from "next-intl"

export function ResourceHeader() {
  const t = useTranslations("Resources")
  return (
    <header className="flex min-h-14 shrink-0 items-center gap-3 border-b px-4 sm:px-6">
      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-foreground text-background">
        <LibraryBig className="size-4" aria-hidden="true" />
      </span>
      <h1 className="text-sm font-semibold">{t("title")}</h1>
    </header>
  )
}

export function ResourceHeading({
  total,
  sessionCount,
  todayCount,
  loading,
  filtered,
}: {
  total: number
  sessionCount: number
  todayCount: number
  loading: boolean
  filtered: boolean
}) {
  const t = useTranslations("Resources")
  return (
    <div className="flex flex-wrap items-start gap-x-5 gap-y-4 pt-7 pb-5">
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex min-w-0 items-center gap-3">
          <h2 className="text-xl font-semibold">{t("deliverables")}</h2>
          {!loading && (
            <span className="rounded-md bg-muted px-2 py-0.5 text-xs tabular-nums text-muted-foreground">
              {t("count", { count: total })}
            </span>
          )}
        </div>
        <p className="text-xs text-muted-foreground">{t("subtitle")}</p>
      </div>
      <div className="flex shrink-0 items-start gap-5">
        <ResourceStat value={sessionCount} label={t("sessionCount")} />
        <ResourceStat value={todayCount} label={t("todayCount")} />
      </div>
      <span className="flex basis-full items-center gap-1.5 text-xs text-muted-foreground">
        <PackageCheck className="size-3.5" aria-hidden="true" />
        {filtered ? t("filteredResults") : t("allConversations")}
      </span>
    </div>
  )
}

function ResourceStat({ value, label }: { value: number; label: string }) {
  return (
    <span className="flex flex-col gap-0.5 text-xs text-muted-foreground">
      <strong className="text-sm font-semibold leading-5 text-foreground tabular-nums">
        {value}
      </strong>
      <span className="whitespace-nowrap">{label}</span>
    </span>
  )
}
