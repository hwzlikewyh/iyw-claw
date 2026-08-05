"use client"

import {
  ArrowLeft,
  Building2,
  Clock,
  FolderTree,
  Package,
  Pencil,
  RotateCcw,
  ShieldAlert,
  Trash2,
  Wrench,
} from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { MarketBadge, MarketBadgeGroup } from "@/components/skills/market/badges"
import { SkillMarketFilesTree } from "@/components/skills/market/files-tree"
import type {
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Version,
} from "@/lib/skill-market"
import {
  artifactStatusBadgeInfo,
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  distributionBadgeInfo,
  formatSkillBytes,
  installStateBadgeInfo,
  primaryInstallAction,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

function formatDate(locale: string, value: string | null): string {
  if (!value) return "—"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "—"
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}

function DetailSkeleton() {
  return (
    <div className="space-y-3 p-4">
      <Skeleton className="h-8 w-2/3" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  )
}

function DetailError({
  error,
  onRetry,
}: {
  error: string
  onRetry: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <p className="text-sm font-medium">{t("detail.error")}</p>
      <p className="max-w-full break-words text-xs text-muted-foreground">
        {error}
      </p>
      <Button size="sm" variant="outline" onClick={onRetry}>
        <RotateCcw className="size-3.5" aria-hidden="true" />
        {t("detail.retry")}
      </Button>
    </div>
  )
}

function SectionLabel({
  icon,
  children,
}: {
  icon?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <h3 className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground">
      {icon}
      {children}
    </h3>
  )
}

function InfoRow({
  label,
  children,
}: {
  label: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-start justify-between gap-3 text-xs">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words text-right">{children}</span>
    </div>
  )
}

export interface SkillMarketDetailProps {
  detail: SkillMarketV2Detail | null
  versions: SkillMarketV2Version[]
  versionsLoading: boolean
  selectedVersion: string | null
  loading: boolean
  error: string | null
  files: {
    value: SkillMarketV2FileNode[] | null
    loading: boolean
    error: string | null
    requested: boolean
  }
  onSelectVersion: (version: string) => void
  onOpenFiles: () => void
  onRetry: () => void
  onBack: () => void
  onPrimaryAction: (detail: SkillMarketV2Detail, version: string) => void
  onEditMetadata: (detail: SkillMarketV2Detail) => void
  onDelete: (detail: SkillMarketV2Detail) => void
  onUninstall: (detail: SkillMarketV2Detail) => void
  onRebuildArtifact: (detail: SkillMarketV2Detail) => void
}

export function SkillMarketDetail(props: SkillMarketDetailProps) {
  const t = useTranslations("SkillMarketV2")
  const locale = useLocale()

  const selectedVersionInfo = props.detail
    ? props.selectedVersion
      ? (props.versions.find((item) => item.version === props.selectedVersion) ??
        props.detail.currentVersion)
      : props.detail.currentVersion
    : null

  if (props.loading) return <DetailSkeleton />
  if (props.error) return <DetailError error={props.error} onRetry={props.onRetry} />
  if (!props.detail) {
    return (
      <div className="flex h-full items-center justify-center p-6 text-center text-sm text-muted-foreground">
        {t("detail.selectHint")}
      </div>
    )
  }

  const detail = props.detail
  const activeVersion = selectedVersionInfo ?? detail.currentVersion
  const action = primaryInstallAction(detail.installState, detail.compatibility)
  const artifactReady = activeVersion.status === "ready"
  // `unknown` compatibility stays installable (optimistic release): the server
  // rejects with `client_incompatible` if it really is, and `installErrorAction`
  // maps that to the update-client recovery path.
  const primaryDisabled =
    action === "none" || !artifactReady || detail.compatibility === "incompatible"
  const primaryKey =
    action === "none"
      ? "none"
      : !artifactReady
        ? activeVersion.status === "artifact_pending"
          ? "waitingArtifact"
          : "buildFailed"
        : action
  const badges = [
    audienceBadgeInfo(detail.audience),
    distributionBadgeInfo(detail.distributionPolicy),
    installStateBadgeInfo(detail.installState),
    ...(detail.compatibility !== "compatible"
      ? [compatibilityBadgeInfo(detail.compatibility)]
      : []),
  ]

  return (
    <div className="h-full min-h-0 overflow-y-auto">
      <div className="flex flex-col gap-4 p-4">
        <div className="flex items-start gap-3">
          <Button
            size="icon-sm"
            variant="ghost"
            className="lg:hidden"
            aria-label={t("a11y.backToList")}
            title={t("a11y.backToList")}
            onClick={props.onBack}
          >
            <ArrowLeft className="size-4" aria-hidden="true" />
          </Button>
          <Avatar className="size-10 shrink-0 rounded-md">
            {detail.iconUrl ? (
              <AvatarImage
                className="rounded-md"
                src={detail.iconUrl}
                alt=""
                loading="lazy"
              />
            ) : null}
            <AvatarFallback className="rounded-md">
              <Package className="size-4" aria-hidden="true" />
            </AvatarFallback>
          </Avatar>
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-base font-semibold">
              {detail.displayName}
            </h2>
            <div className="mt-1 flex flex-wrap items-center gap-1.5">
              <MarketBadgeGroup badges={badges} limit={4} />
            </div>
          </div>
        </div>

        <div className="grid gap-2 sm:grid-cols-[1fr_auto]">
          <Select
            value={activeVersion.version}
            onValueChange={props.onSelectVersion}
            disabled={props.versionsLoading}
          >
            <SelectTrigger className="w-full rounded-md">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {props.versions.map((item) => (
                <SelectItem
                  key={item.id}
                  value={item.version}
                  disabled={item.status !== "ready"}
                >
                  v{item.version}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <div className="flex items-center gap-2">
            <Button
              disabled={primaryDisabled}
              title={
                action === "none"
                  ? t("list.primary.noneHint")
                  : !artifactReady
                    ? t("detail.artifactNotReadyHint")
                    : undefined
              }
              onClick={() => props.onPrimaryAction(detail, activeVersion.version)}
            >
              {t(`list.primary.${primaryKey}`)}
            </Button>
            {detail.canManage ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    size="icon-sm"
                    variant="outline"
                    aria-label={t("manage.editMetadata")}
                    title={t("manage.editMetadata")}
                    onClick={() => props.onEditMetadata(detail)}
                  >
                    <Pencil className="size-3.5" aria-hidden="true" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("manage.editMetadata")}</TooltipContent>
              </Tooltip>
            ) : null}
            {detail.installState !== "not_installed" ? (
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    size="icon-sm"
                    variant="outline"
                    className="text-destructive"
                    aria-label={t("manage.uninstall")}
                    title={t("manage.uninstall")}
                    onClick={() => props.onUninstall(detail)}
                  >
                    <Trash2 className="size-3.5" aria-hidden="true" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>{t("manage.uninstall")}</TooltipContent>
              </Tooltip>
            ) : null}
          </div>
        </div>

        <section className="space-y-2">
          <SectionLabel>{t("detail.overview")}</SectionLabel>
          <p className="text-xs leading-5 text-muted-foreground">
            {detail.summary}
          </p>
          <div className="flex flex-wrap gap-1.5">
            {detail.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
              >
                {tag}
              </span>
            ))}
          </div>
          <div className="grid gap-1.5">
            <InfoRow label={t("detail.updated")}>
              {formatDate(locale, detail.updatedAt)}
            </InfoRow>
            {detail.organizationName ? (
              <InfoRow label={t("detail.organization")}>
                {detail.organizationName}
              </InfoRow>
            ) : null}
          </div>
        </section>

        <Separator />

        <section className="space-y-2">
          <SectionLabel icon={<ShieldAlert className="size-3.5" aria-hidden="true" />}>
            {t("detail.compatibility")}
          </SectionLabel>
          <div className="grid gap-1.5">
            <InfoRow label={t("detail.clientVersion")}>
              {detail.compatibilityDetail.minClientVersion ?? "—"}
            </InfoRow>
            <InfoRow label={t("detail.osArch")}>
              {detail.compatibilityDetail.osArch ?? "—"}
            </InfoRow>
            {detail.compatibilityDetail.reason ? (
              <InfoRow label={t("detail.mandatoryReason")}>
                {detail.compatibilityDetail.reason}
              </InfoRow>
            ) : null}
            {detail.compatibilityDetail.deadline ? (
              <InfoRow label={t("detail.deadline")}>
                {formatDate(locale, detail.compatibilityDetail.deadline)}
              </InfoRow>
            ) : null}
          </div>
          <div className="pt-1">
            <SectionLabel>{t("detail.dependencies")}</SectionLabel>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              {activeVersion.dependencies.length ? (
                activeVersion.dependencies.map((dependency) => (
                  <span
                    key={dependency.skillId}
                    className="rounded-md border px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground"
                  >
                    {dependency.slug}@{dependency.version}
                  </span>
                ))
              ) : (
                <span className="text-xs text-muted-foreground">
                  {t("detail.noDependencies")}
                </span>
              )}
            </div>
          </div>
        </section>

        <Separator />

        <section className="space-y-2">
          <SectionLabel>{t("detail.versions")}</SectionLabel>
          <div className="space-y-1.5">
            {props.versions.map((item) => {
              const selected = item.version === activeVersion.version
              return (
                <div
                  key={item.id}
                  className={cn(
                    "rounded-md border px-2.5 py-2",
                    selected ? "border-primary/40 bg-primary/5" : ""
                  )}
                >
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left font-mono text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                      onClick={() => props.onSelectVersion(item.version)}
                      disabled={item.status !== "ready"}
                      title={
                        item.status !== "ready"
                          ? t("detail.versionUnavailable")
                          : undefined
                      }
                    >
                      v{item.version}
                    </button>
                    <MarketBadge info={artifactStatusBadgeInfo(item.status)} />
                    <span className="shrink-0 text-[10px] text-muted-foreground">
                      {formatSkillBytes(item.artifactSize)}
                    </span>
                    <span className="shrink-0 text-[10px] text-muted-foreground">
                      {formatDate(locale, item.releasedAt)}
                    </span>
                    {item.status === "failed" ? (
                      <Button
                        size="xs"
                        variant="outline"
                        onClick={() => props.onRebuildArtifact(detail)}
                      >
                        <Wrench className="size-3" aria-hidden="true" />
                        {t("manage.rebuildArtifact")}
                      </Button>
                    ) : null}
                  </div>
                  {item.changelog ? (
                    <Collapsible>
                      <CollapsibleTrigger asChild>
                        <Button
                          size="xs"
                          variant="ghost"
                          className="mt-1 h-5 px-1 text-[10px] text-muted-foreground"
                        >
                          {t("detail.releaseNotes")}
                        </Button>
                      </CollapsibleTrigger>
                      <CollapsibleContent>
                        <p className="mt-1 whitespace-pre-wrap break-words rounded-md bg-muted/40 px-2 py-1.5 text-[11px] leading-5">
                          {item.changelog}
                        </p>
                      </CollapsibleContent>
                    </Collapsible>
                  ) : null}
                </div>
              )
            })}
          </div>
        </section>

        <Separator />

        <section className="space-y-2">
          <SectionLabel icon={<FolderTree className="size-3.5" aria-hidden="true" />}>
            {t("detail.files")}
          </SectionLabel>
          {props.files.requested ? (
            <SkillMarketFilesTree
              files={props.files.value ?? []}
              loading={props.files.loading}
              error={props.files.error}
              onRetry={props.onOpenFiles}
            />
          ) : (
            <Button
              size="sm"
              variant="outline"
              onClick={props.onOpenFiles}
            >
              <FolderTree className="size-3.5" aria-hidden="true" />
              {t("detail.loadFiles")}
            </Button>
          )}
        </section>

        <Separator />

        <section className="space-y-2">
          <SectionLabel icon={<Building2 className="size-3.5" aria-hidden="true" />}>
            {t("detail.ownership")}
          </SectionLabel>
          <div className="grid gap-1.5">
            <InfoRow label={t("detail.ownershipSource")}>
              {t(`detail.ownershipSourceValue.${detail.ownership.source}`)}
            </InfoRow>
            <InfoRow label={t("detail.managed")}>
              {detail.ownership.managed
                ? t("detail.managedYes")
                : t("detail.managedNo")}
            </InfoRow>
          </div>
          {detail.ownership.managed ? (
            <p className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
              <Clock className="size-3" aria-hidden="true" />
              {t("detail.pathHidden")}
            </p>
          ) : null}
        </section>
      </div>
    </div>
  )
}
