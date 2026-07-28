"use client"

import { FolderOpen, Loader2, Package, Power, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { compareSemVer } from "@/components/skills/skill-market-semver"
import {
  getInstalledMarketInfo,
  type SkillMarketDetail,
} from "@/lib/skill-market"
import type { AgentSkillItem } from "@/lib/types"
import { cn } from "@/lib/utils"

function InstalledLoading() {
  return (
    <div className="space-y-2">
      {Array.from({ length: 4 }, (_, index) => (
        <Skeleton key={index} className="h-20 rounded-lg" />
      ))}
    </div>
  )
}

function InstalledEmpty() {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="rounded-lg border border-dashed px-4 py-10 text-center">
      <Package className="mx-auto size-6 text-muted-foreground" />
      <p className="mt-3 text-sm font-medium">{t("installed.emptyTitle")}</p>
      <p className="mt-1 text-xs text-muted-foreground">
        {t("installed.emptyDescription")}
      </p>
    </div>
  )
}

function InstalledBadges({
  skill,
  managed,
  hasUpdate,
}: {
  skill: AgentSkillItem
  managed: boolean
  hasUpdate: boolean
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <span className="flex min-w-0 flex-wrap items-center gap-1.5">
      <span className="truncate text-sm font-medium">{skill.name}</span>
      <Badge
        variant={managed ? "outline" : "secondary"}
        className="h-5 px-1.5 text-[10px]"
      >
        {t(managed ? "installed.managed" : "installed.local")}
      </Badge>
      {hasUpdate ? (
        <Badge className="h-5 px-1.5 text-[10px]">
          {t("installed.updateAvailable")}
        </Badge>
      ) : null}
      {!skill.enabled ? (
        <Badge variant="outline" className="h-5 px-1.5 text-[10px]">
          {t("installed.disabled")}
        </Badge>
      ) : null}
    </span>
  )
}

type InstalledSkillCardProps = {
  skill: AgentSkillItem
  remote: SkillMarketDetail | null
  selected: boolean
  onSelect: () => void
}

function InstalledSkillCard(props: InstalledSkillCardProps) {
  const { skill, remote, selected, onSelect } = props
  const t = useTranslations("SkillsSettings.market")
  const market = getInstalledMarketInfo(skill)
  const hasUpdate = Boolean(
    market.version &&
    remote &&
    compareSemVer(remote.currentVersion.version, market.version) > 0
  )
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "w-full min-w-0 rounded-lg border bg-card p-3 text-left transition-colors",
        selected ? "border-primary/50 bg-primary/5" : "hover:bg-muted/20"
      )}
    >
      <span className="flex min-w-0 items-start gap-3">
        <span className="flex size-9 shrink-0 items-center justify-center rounded-md border bg-background text-muted-foreground">
          <Package className="size-4" />
        </span>
        <span className="min-w-0 flex-1">
          <InstalledBadges
            skill={skill}
            managed={market.managed}
            hasUpdate={hasUpdate}
          />
          <span className="mt-1 line-clamp-2 text-xs text-muted-foreground">
            {skill.description || t("installed.noDescription")}
          </span>
          {market.version ? (
            <span className="mt-2 block font-mono text-[11px] text-muted-foreground">
              v{market.version}
              {remote ? ` -> ${remote.currentVersion.version}` : ""}
            </span>
          ) : null}
        </span>
      </span>
    </button>
  )
}

export function SkillMarketInstalledList({
  skills,
  remoteById,
  selectedId,
  loading,
  onSelect,
}: {
  skills: AgentSkillItem[]
  remoteById: Map<string, SkillMarketDetail>
  selectedId: string | null
  loading: boolean
  onSelect: (skill: AgentSkillItem) => void
}) {
  if (loading) return <InstalledLoading />
  if (!skills.length) return <InstalledEmpty />
  return (
    <div className="space-y-2">
      {skills.map((skill) => {
        const market = getInstalledMarketInfo(skill)
        const remote = market.marketId ? remoteById.get(market.marketId) : null
        return (
          <InstalledSkillCard
            key={`${skill.scope}:${skill.id}`}
            skill={skill}
            remote={remote ?? null}
            selected={selectedId === skill.id}
            onSelect={() => onSelect(skill)}
          />
        )
      })}
    </div>
  )
}

function LocalSkillToggle({
  skill,
  toggling,
  onToggle,
}: {
  skill: AgentSkillItem
  toggling: boolean
  onToggle: (enabled: boolean) => void
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="mt-5 flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
      <div>
        <div className="flex items-center gap-1.5 text-xs font-medium">
          <Power className="size-3.5" />
          {t("installed.enabled")}
        </div>
        <p className="mt-1 text-[11px] text-muted-foreground">
          {t("installed.enabledHint")}
        </p>
      </div>
      {toggling ? (
        <Loader2 className="size-4 animate-spin" />
      ) : (
        <Switch checked={skill.enabled} onCheckedChange={onToggle} />
      )}
    </div>
  )
}

function LocalSkillActions({
  deleting,
  onOpenFolder,
  onDelete,
}: {
  deleting: boolean
  onOpenFolder: () => void
  onDelete: () => void
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="mt-4 flex flex-wrap gap-2">
      <Button variant="outline" size="sm" onClick={onOpenFolder}>
        <FolderOpen className="size-3.5" />
        {t("actions.openFolder")}
      </Button>
      <Button
        variant="outline"
        size="sm"
        className="text-destructive"
        disabled={deleting}
        onClick={onDelete}
      >
        <Trash2 className="size-3.5" />
        {deleting ? t("actions.deleting") : t("actions.uninstall")}
      </Button>
    </div>
  )
}

export function LocalSkillDetail({
  skill,
  toggling,
  deleting,
  onOpenFolder,
  onToggle,
  onDelete,
}: {
  skill: AgentSkillItem | null
  toggling: boolean
  deleting: boolean
  onOpenFolder: () => void
  onToggle: (enabled: boolean) => void
  onDelete: () => void
}) {
  const t = useTranslations("SkillsSettings.market")
  if (!skill) {
    return (
      <div className="flex min-h-48 items-center justify-center rounded-lg border border-dashed px-5 text-center text-sm text-muted-foreground">
        {t("detail.selectHint")}
      </div>
    )
  }
  return (
    <aside className="rounded-lg border bg-card p-4 md:sticky md:top-4 md:self-start">
      <div className="flex items-start gap-3">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-md border">
          <Package className="size-4" />
        </span>
        <div className="min-w-0">
          <h2 className="break-words text-base font-semibold">{skill.name}</h2>
          <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
            {skill.id}
          </p>
        </div>
      </div>
      <p className="mt-4 text-sm leading-6 text-muted-foreground">
        {skill.description || t("installed.noDescription")}
      </p>
      <LocalSkillToggle skill={skill} toggling={toggling} onToggle={onToggle} />
      <LocalSkillActions
        deleting={deleting}
        onOpenFolder={onOpenFolder}
        onDelete={onDelete}
      />
    </aside>
  )
}

export { SkillMarketUninstallDialog } from "@/components/skills/skill-market-uninstall-dialog"
