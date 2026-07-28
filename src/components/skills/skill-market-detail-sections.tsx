"use client"

import {
  BadgeCheck,
  FileText,
  FolderTree,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { SkillMarketDetail, SkillMarketVersion } from "@/lib/skill-market"

type DetailActions = {
  onEdit?: () => void
  onAddVersion?: () => void
  onDelete?: () => void
}

function ManageActions(props: DetailActions) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="flex shrink-0 items-center gap-1">
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={t("actions.editMetadata")}
        title={t("actions.editMetadata")}
        onClick={props.onEdit}
      >
        <Pencil className="size-3.5" aria-hidden="true" />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        aria-label={t("actions.publishVersion")}
        title={t("actions.publishVersion")}
        onClick={props.onAddVersion}
      >
        <Plus className="size-3.5" aria-hidden="true" />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        className="text-destructive"
        aria-label={t("actions.delete")}
        title={t("actions.delete")}
        onClick={props.onDelete}
      >
        <Trash2 className="size-3.5" aria-hidden="true" />
      </Button>
    </div>
  )
}

export function DetailHeader({
  detail,
  actions,
}: {
  detail: SkillMarketDetail
  actions: DetailActions
}) {
  const t = useTranslations("SkillsSettings.market")
  const showManageActions =
    detail.canManage &&
    Boolean(actions.onEdit || actions.onAddVersion || actions.onDelete)
  return (
    <>
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <h2 className="break-words text-base font-semibold">
              {detail.displayName}
            </h2>
            {detail.publisherType === "official" ? (
              <Badge variant="outline" className="gap-1">
                <BadgeCheck className="size-3" />
                {t("publisher.official")}
              </Badge>
            ) : null}
            <Badge variant="secondary">
              {t(`visibility.${detail.visibility}`)}
            </Badge>
          </div>
          <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
            {detail.slug}
          </p>
        </div>
        {showManageActions ? <ManageActions {...actions} /> : null}
      </div>
      <p className="mt-3 text-sm leading-6 text-muted-foreground">
        {detail.summary}
      </p>
      <div className="mt-3 flex flex-wrap gap-1.5">
        {detail.tags.map((tag) => (
          <Badge key={tag} variant="outline" className="text-[10px]">
            {tag}
          </Badge>
        ))}
      </div>
    </>
  )
}

export function VersionChangelog({ version }: { version: SkillMarketVersion }) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <section className="mt-5">
      <h3 className="text-xs font-semibold">{t("detail.changelog")}</h3>
      <p className="mt-2 whitespace-pre-wrap break-words text-xs leading-5 text-muted-foreground">
        {version.changelog || t("detail.noChangelog")}
      </p>
    </section>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function DetailFileTree({ detail }: { detail: SkillMarketDetail }) {
  const t = useTranslations("SkillsSettings.market")
  const files = [...(detail.files ?? [])].sort((a, b) =>
    a.path.localeCompare(b.path)
  )
  return (
    <section className="mt-5">
      <div className="flex items-center justify-between gap-2">
        <h3 className="flex items-center gap-1.5 text-xs font-semibold">
          <FolderTree className="size-3.5" />
          {t("detail.fileTree")}
        </h3>
        <span className="text-[11px] text-muted-foreground">
          {t("detail.fileCount", { count: files.length })}
        </span>
      </div>
      <div className="mt-2 max-h-64 overflow-auto rounded-md border bg-muted/10 p-2">
        {files.length ? (
          files.map((file) => (
            <div
              key={file.path}
              className="flex min-w-0 items-center gap-2 px-1 py-1 text-[11px]"
            >
              <FileText className="size-3 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 break-all font-mono">
                {file.path}
              </span>
              <span className="shrink-0 text-muted-foreground">
                {formatBytes(file.size)}
              </span>
            </div>
          ))
        ) : (
          <p className="p-2 text-xs text-muted-foreground">
            {t("detail.noFiles")}
          </p>
        )}
      </div>
    </section>
  )
}
