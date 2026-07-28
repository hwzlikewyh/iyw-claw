"use client"

import { useState } from "react"
import { RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import {
  DetailFileTree,
  DetailHeader,
  VersionChangelog,
} from "@/components/skills/skill-market-detail-sections"
import {
  InstallConfirm,
  InstallControls,
  getInstallAction,
} from "@/components/skills/skill-market-install-controls"
import type {
  SkillMarketDetail as Detail,
  SkillMarketVersion,
} from "@/lib/skill-market"

function DetailLoading() {
  return (
    <div className="space-y-4 rounded-lg border bg-card p-4">
      <Skeleton className="h-7 w-2/3" />
      <Skeleton className="h-16 w-full" />
      <Skeleton className="h-40 w-full" />
    </div>
  )
}

function DetailUnavailable() {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="flex min-h-48 items-center justify-center rounded-lg border border-dashed px-5 text-center text-sm text-muted-foreground">
      {t("detail.selectHint")}
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
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-5 text-center">
      <p className="text-sm font-medium">{t("detail.loadFailed")}</p>
      <p className="mt-1 break-words text-xs text-muted-foreground">{error}</p>
      <Button size="sm" variant="outline" className="mt-4" onClick={onRetry}>
        <RotateCcw className="size-3.5" />
        {t("actions.retry")}
      </Button>
    </div>
  )
}

type SkillMarketDetailProps = {
  detail: Detail | null
  versions: SkillMarketVersion[]
  installedVersion: string | null
  loading: boolean
  detailError: string | null
  versionsLoading: boolean
  versionsError: string | null
  busy: boolean
  installDisabled: boolean
  onVersionChange: (version: string) => void
  onRetryDetail: () => void
  onRetryVersions: () => void
  onInstall: (version: string) => void
  onEdit?: () => void
  onAddVersion?: () => void
  onDelete?: () => void
  onUninstall?: () => void
}

export function SkillMarketDetail(props: SkillMarketDetailProps) {
  const [confirmOpen, setConfirmOpen] = useState(false)
  if (props.loading) return <DetailLoading />
  if (props.detailError) {
    return (
      <DetailError error={props.detailError} onRetry={props.onRetryDetail} />
    )
  }
  if (!props.detail) return <DetailUnavailable />
  const version = props.detail.currentVersion.version
  const action = getInstallAction(props.installedVersion, version)
  const versions = props.versions.some((item) => item.version === version)
    ? props.versions
    : [props.detail.currentVersion, ...props.versions]
  const selectedVersion =
    versions.find((item) => item.version === version) ??
    props.detail.currentVersion
  const installable = selectedVersion.status === "ready"
  return (
    <aside className="min-w-0 rounded-lg border bg-card p-4 md:sticky md:top-4 md:self-start">
      <DetailHeader detail={props.detail} actions={props} />
      <InstallControls
        {...props}
        versions={versions}
        version={version}
        action={action}
        installable={installable}
        onConfirm={() => setConfirmOpen(true)}
      />
      <VersionChangelog version={selectedVersion} />
      <DetailFileTree detail={props.detail} />
      <InstallConfirm
        open={confirmOpen}
        action={action}
        current={props.installedVersion}
        target={version}
        onOpenChange={setConfirmOpen}
        onInstall={() => props.onInstall(version)}
      />
    </aside>
  )
}
