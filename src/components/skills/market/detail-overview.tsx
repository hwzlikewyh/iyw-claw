"use client"

import { useLocale, useTranslations } from "next-intl"
import {
  formatSkillBytes,
  type SkillMarketAudience,
  type SkillMarketTranslator,
  type SkillMarketV2Detail,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import {
  PluginComponentSummary,
  PluginPermissionSummary,
} from "./detail-overview-plugin"

function formatDate(locale: string, value: string | null): string {
  if (!value) return "-"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "-"
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}

function audienceKey(audience: SkillMarketAudience) {
  switch (audience) {
    case "global_market":
      return "globalMarket"
    case "organization":
      return "organization"
    case "owner_private":
      return "ownerPrivate"
  }
}

function Fact({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-3 py-2 text-xs">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="break-words text-right font-medium">{value}</dd>
    </div>
  )
}

function ScopeSummary({ detail }: { detail: SkillMarketV2Detail }) {
  const messages = useTranslations("SkillMarketV2")
  const t = messages as unknown as SkillMarketTranslator
  const categoryKey = `categories.${detail.category}` as Parameters<
    typeof messages
  >[0]
  return (
    <dl className="divide-y border-y">
      <Fact
        label={t("detail.publisher")}
        value={detail.organizationName || t(`filters.${detail.publisher}`)}
      />
      <Fact
        label={t("detail.category")}
        value={
          detail.category
            ? messages.has(categoryKey)
              ? t(categoryKey)
              : detail.category
            : "-"
        }
      />
      <Fact
        label={t("detail.publicationScope")}
        value={t(`audience.${audienceKey(detail.audience)}`)}
      />
      <Fact
        label={t("filters.distribution")}
        value={t(`distribution.${detail.distributionPolicy}`)}
      />
    </dl>
  )
}

function Metric({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="min-w-0 py-3 pr-3 last:pr-0 sm:border-r sm:px-3 sm:first:pl-0 sm:last:border-r-0">
      <p className="text-[10px] text-muted-foreground">{label}</p>
      <div className="mt-1 truncate text-xs font-semibold">{value}</div>
    </div>
  )
}

function AboutSummary({ detail }: { detail: SkillMarketV2Detail }) {
  const t = useTranslations("SkillMarketV2")
  const catalog = useTranslations("CapabilityMarket.catalog")
  return (
    <section>
      <h3 className="text-sm font-semibold">
        {t(detail.packageType === "plugin" ? "detail.useCase" : "detail.about")}
      </h3>
      <p className="mt-2 text-sm leading-6 text-muted-foreground">
        {detail.summary || catalog("noDescription")}
      </p>
      {detail.tags.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-1.5">
          {detail.tags.map((tag) => (
            <span
              key={tag}
              className="rounded-md bg-muted px-2 py-1 text-[10px] text-muted-foreground"
            >
              {tag}
            </span>
          ))}
        </div>
      )}
    </section>
  )
}

function VersionMetrics({
  detail,
  version,
}: {
  detail: SkillMarketV2Detail
  version: SkillMarketV2Version
}) {
  const t = useTranslations("SkillMarketV2")
  const locale = useLocale()
  return (
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
  )
}

function ReleaseNotes({ version }: { version: SkillMarketV2Version }) {
  const t = useTranslations("SkillMarketV2")
  return (
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
  )
}

function PackageInformation({
  detail,
  version,
}: {
  detail: SkillMarketV2Detail
  version: SkillMarketV2Version
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <section>
      <h3 className="text-sm font-semibold">{t("detail.packageInfo")}</h3>
      <dl className="mt-2 divide-y border-y">
        <Fact
          label={t("install.downloadSize")}
          value={formatSkillBytes(version.artifactSize)}
        />
        <Fact
          label={t("detail.ownership")}
          value={t(`detail.ownershipSourceValue.${detail.ownership.source}`)}
        />
      </dl>
    </section>
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
  return (
    <div className="space-y-5">
      <AboutSummary detail={detail} />
      {detail.packageType === "plugin" && (
        <>
          <section>
            <h3 className="mb-2 text-xs font-semibold">
              {t("detail.components")}
            </h3>
            <PluginComponentSummary version={version} />
          </section>
          <section>
            <h3 className="mb-2 text-xs font-semibold">
              {t("detail.permissions")}
            </h3>
            <PluginPermissionSummary version={version} />
          </section>
        </>
      )}
      <section>
        <h3 className="mb-2 text-xs font-semibold">
          {t("detail.publicationScope")}
        </h3>
        <ScopeSummary detail={detail} />
      </section>
      <VersionMetrics detail={detail} version={version} />
      <ReleaseNotes version={version} />
      <PackageInformation detail={detail} version={version} />
    </div>
  )
}
