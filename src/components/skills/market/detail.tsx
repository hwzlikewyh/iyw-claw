"use client"

import { ArrowLeft, Package, Pencil, RotateCcw, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import { DetailInspectorTabs } from "@/components/skills/market/detail-inspector-tabs"
import type {
  SkillMarketV2Detail,
  SkillMarketV2FileNode,
  SkillMarketV2Version,
} from "@/lib/skill-market"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  installStateBadgeInfo,
  primaryInstallAction,
} from "@/lib/skill-market"

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
  onRebuildArtifact: (detail: SkillMarketV2Detail, version: string) => void
}

function DetailState({
  loading,
  error,
  onRetry,
}: {
  loading: boolean
  error: string | null
  onRetry: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  if (loading) {
    return (
      <div className="space-y-3 p-4">
        <Skeleton className="h-9 w-2/3" />
        <Skeleton className="h-24 w-full" />
        <Skeleton className="h-52 w-full" />
      </div>
    )
  }
  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <p className="text-sm font-medium">{t("detail.error")}</p>
        <p className="break-words text-xs text-muted-foreground">{error}</p>
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RotateCcw className="size-3.5" aria-hidden="true" />
          {t("detail.retry")}
        </Button>
      </div>
    )
  }
  return (
    <div className="flex h-full items-center justify-center p-6 text-center text-sm text-muted-foreground">
      {t("detail.selectHint")}
    </div>
  )
}

export function SkillMarketDetail(props: SkillMarketDetailProps) {
  const t = useTranslations("SkillMarketV2")
  if (props.loading || props.error || !props.detail) {
    return (
      <DetailState
        loading={props.loading}
        error={props.error}
        onRetry={props.onRetry}
      />
    )
  }
  const detail = props.detail
  const activeVersion = props.selectedVersion
    ? (props.versions.find((item) => item.version === props.selectedVersion) ??
      detail.currentVersion)
    : detail.currentVersion
  const action = primaryInstallAction(detail.installState, detail.compatibility)
  const artifactReady = activeVersion.status === "ready"
  const primaryDisabled =
    action === "none" ||
    !artifactReady ||
    detail.compatibility === "incompatible"
  const primaryKey = !artifactReady
    ? activeVersion.status === "artifact_pending"
      ? "waitingArtifact"
      : "buildFailed"
    : action
  const badges = [
    installStateBadgeInfo(detail.installState),
    audienceBadgeInfo(detail.audience),
    ...(detail.compatibility !== "compatible"
      ? [compatibilityBadgeInfo(detail.compatibility)]
      : []),
  ]

  return (
    <div className="flex h-full min-h-0 flex-col bg-muted/10">
      <div className="border-b bg-background p-4">
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
              <AvatarImage className="rounded-md" src={detail.iconUrl} alt="" />
            ) : null}
            <AvatarFallback className="rounded-md">
              <Package className="size-4" aria-hidden="true" />
            </AvatarFallback>
          </Avatar>
          <div className="min-w-0 flex-1">
            <h2 className="truncate text-sm font-semibold">
              {detail.displayName}
            </h2>
            <p className="mt-0.5 truncate font-mono text-[10px] text-muted-foreground">
              {detail.slug}
            </p>
            <MarketBadgeGroup badges={badges} limit={3} className="mt-2" />
          </div>
        </div>

        <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_auto]">
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
          <Button
            disabled={primaryDisabled}
            onClick={() => props.onPrimaryAction(detail, activeVersion.version)}
          >
            {t(`list.primary.${primaryKey}`)}
          </Button>
        </div>

        <div className="mt-2 flex justify-end gap-1">
          {detail.canManage ? (
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label={t("manage.editMetadata")}
              title={t("manage.editMetadata")}
              onClick={() => props.onEditMetadata(detail)}
            >
              <Pencil className="size-3.5" />
            </Button>
          ) : null}
          {detail.installState !== "not_installed" ? (
            <Button
              size="icon-sm"
              variant="ghost"
              className="text-destructive"
              aria-label={t("manage.uninstall")}
              title={t("manage.uninstall")}
              onClick={() => props.onUninstall(detail)}
            >
              <Trash2 className="size-3.5" />
            </Button>
          ) : null}
        </div>
      </div>

      <DetailInspectorTabs
        detail={detail}
        activeVersion={activeVersion}
        versions={props.versions}
        files={props.files}
        onOpenFiles={props.onOpenFiles}
        onSelectVersion={props.onSelectVersion}
        onRebuildArtifact={(version) =>
          props.onRebuildArtifact(detail, version)
        }
      />
    </div>
  )
}
