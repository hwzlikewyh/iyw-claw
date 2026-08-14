"use client"

import { useLocale, useTranslations } from "next-intl"
import {
  formatSkillBytes,
  type SkillMarketV2Detail,
  type SkillMarketV2Version,
} from "@/lib/skill-market"

function formatDate(locale: string, value: string | null): string {
  if (!value) return "-"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "-"
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}

function Metric({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="min-w-0 py-3 pr-4 last:pr-0 sm:border-r sm:px-4 sm:first:pl-0 sm:last:border-r-0">
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <div className="mt-1 truncate text-xs font-semibold">{value}</div>
    </div>
  )
}

export function DetailOverview({
  detail,
  version,
}: {
  detail: SkillMarketV2Detail
  version: SkillMarketV2Version
}) {
  const t = useTranslations("SkillMarketV2")
  const locale = useLocale()
  return (
    <div className="space-y-6">
      <section>
        <h3 className="text-sm font-semibold">{t("detail.about")}</h3>
        <p className="mt-2 text-sm leading-7 text-muted-foreground">
          {detail.summary}
        </p>
        {detail.tags.length ? (
          <div className="mt-3 flex flex-wrap gap-1.5">
            {detail.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-sm bg-muted px-2 py-1 text-[10px] text-muted-foreground"
              >
                {tag}
              </span>
            ))}
          </div>
        ) : null}
      </section>
      <section className="grid grid-cols-2 border-y sm:grid-cols-4">
        <Metric
          label={t("detail.currentVersion")}
          value={`v${version.version}`}
        />
        <Metric
          label={t("detail.fileCountLabel")}
          value={t("detail.fileCount", { count: version.fileCount })}
        />
        <Metric
          label={t("detail.dependencies")}
          value={version.dependencies.length}
        />
        <Metric
          label={t("detail.updated")}
          value={formatDate(locale, detail.updatedAt)}
        />
      </section>
      <section>
        <h3 className="text-sm font-semibold">{t("detail.releaseNotes")}</h3>
        <div className="mt-2 grid grid-cols-[4.5rem_minmax(0,1fr)] gap-3 border-t py-3">
          <span className="font-mono text-xs font-semibold text-primary">
            v{version.version}
          </span>
          <p className="text-xs leading-5 text-muted-foreground">
            {version.changelog || t("detail.noReleaseNotes")}
          </p>
        </div>
      </section>
      <section>
        <h3 className="text-sm font-semibold">{t("detail.packageInfo")}</h3>
        <dl className="mt-2 divide-y border-y">
          <div className="flex items-center justify-between gap-4 py-2 text-xs">
            <dt className="text-muted-foreground">
              {t("install.downloadSize")}
            </dt>
            <dd>{formatSkillBytes(version.artifactSize)}</dd>
          </div>
          <div className="flex items-center justify-between gap-4 py-2 text-xs">
            <dt className="text-muted-foreground">{t("detail.ownership")}</dt>
            <dd>
              {t(`detail.ownershipSourceValue.${detail.ownership.source}`)}
            </dd>
          </div>
        </dl>
      </section>
    </div>
  )
}
