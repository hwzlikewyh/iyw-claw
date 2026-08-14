"use client"

import { RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { DetailHeader } from "@/components/skills/market/detail-header"
import { DetailInspectorTabs } from "@/components/skills/market/detail-inspector-tabs"
import { DetailSidePanel } from "@/components/skills/market/detail-side-panel"
import {
  primaryInstallAction,
  type SkillMarketTranslator,
  type SkillMarketV2Detail,
  type SkillMarketV2FileNode,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import type { SkillMarketActivationSummary } from "@/lib/skill-market-activation"

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
  activation: SkillMarketActivationSummary
  activationBusy: boolean
  activationError: string | null
  onSelectVersion: (version: string) => void
  onOpenFiles: () => void
  onRetry: () => void
  onPrimaryAction: (detail: SkillMarketV2Detail, version: string) => void
  onToggleActivation: (enabled: boolean) => void
  onOpenInventory: () => void
  onOpenConnectors: () => void
  onRetryActivation: () => void
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
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  if (loading) {
    return (
      <div className="space-y-3 p-5">
        <Skeleton className="h-12 w-2/3" />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-72 w-full" />
      </div>
    )
  }
  if (error) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
        <p className="text-sm font-medium">{t("detail.error")}</p>
        <p className="break-words text-xs text-muted-foreground">{error}</p>
        <Button size="sm" variant="outline" onClick={onRetry}>
          <RotateCcw className="size-3.5" />
          {t("detail.retry")}
        </Button>
      </div>
    )
  }
  return (
    <div className="flex h-full items-center justify-center p-6 text-sm text-muted-foreground">
      {t("detail.selectHint")}
    </div>
  )
}

export function SkillMarketDetail(props: SkillMarketDetailProps) {
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
  const versions = props.versions.some(
    (item) => item.version === detail.currentVersion.version
  )
    ? props.versions
    : [detail.currentVersion, ...props.versions]
  const action = primaryInstallAction(detail.installState, detail.compatibility)
  const artifactReady = activeVersion.status === "ready"
  const primaryDisabled =
    props.activationBusy ||
    action === "none" ||
    !artifactReady ||
    detail.compatibility === "incompatible"
  const primaryKey = !artifactReady
    ? activeVersion.status === "artifact_pending"
      ? "waitingArtifact"
      : "buildFailed"
    : action
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <DetailHeader
        detail={detail}
        activeVersion={activeVersion}
        versions={versions}
        versionsLoading={props.versionsLoading}
        activation={props.activation}
        activationBusy={props.activationBusy}
        primaryKey={primaryKey}
        primaryDisabled={primaryDisabled}
        onSelectVersion={props.onSelectVersion}
        onPrimaryAction={() =>
          props.onPrimaryAction(detail, activeVersion.version)
        }
        onToggleActivation={props.onToggleActivation}
        onEditMetadata={() => props.onEditMetadata(detail)}
        onDelete={() => props.onDelete(detail)}
      />
      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto lg:grid-cols-[minmax(0,1fr)_19rem] lg:overflow-hidden">
        <DetailInspectorTabs
          detail={detail}
          activeVersion={activeVersion}
          versions={versions}
          files={props.files}
          onOpenFiles={props.onOpenFiles}
          onSelectVersion={props.onSelectVersion}
          onRebuildArtifact={(version) =>
            props.onRebuildArtifact(detail, version)
          }
        />
        <DetailSidePanel
          detail={detail}
          version={activeVersion}
          activation={props.activation}
          activationBusy={props.activationBusy}
          activationError={props.activationError}
          onEnableAll={() => props.onToggleActivation(true)}
          onOpenInventory={props.onOpenInventory}
          onOpenConnectors={props.onOpenConnectors}
          onRetryActivation={props.onRetryActivation}
          onUninstall={() => props.onUninstall(detail)}
        />
      </div>
    </div>
  )
}
