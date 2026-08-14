"use client"

import { Loader2, MoreHorizontal, Package, Pencil, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { MarketBadgeGroup } from "@/components/skills/market/badges"
import {
  audienceBadgeInfo,
  compatibilityBadgeInfo,
  installStateBadgeInfo,
  type SkillMarketTranslator,
  type SkillMarketV2Detail,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import type { SkillMarketActivationSummary } from "@/lib/skill-market-activation"
import { cn } from "@/lib/utils"

interface DetailHeaderProps {
  detail: SkillMarketV2Detail
  activeVersion: SkillMarketV2Version
  versions: SkillMarketV2Version[]
  versionsLoading: boolean
  activation: SkillMarketActivationSummary
  activationBusy: boolean
  primaryKey: string
  primaryDisabled: boolean
  onSelectVersion: (version: string) => void
  onPrimaryAction: () => void
  onToggleActivation: (enabled: boolean) => void
  onEditMetadata: () => void
  onDelete: () => void
}

function ActivationControl({
  activation,
  busy,
  onToggle,
}: {
  activation: SkillMarketActivationSummary
  busy: boolean
  onToggle: (enabled: boolean) => void
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const checked = activation.kind === "active" || activation.kind === "partial"
  const interactive = ["inactive", "partial", "active"].includes(
    activation.kind
  )
  const disabled =
    busy ||
    !interactive ||
    (checked ? !activation.canDisable : !activation.canEnable)
  return (
    <section
      className={cn(
        "flex min-h-16 w-full items-center gap-3 rounded-md border px-3 py-2.5 sm:w-72",
        activation.kind === "active" &&
          "border-emerald-500/30 bg-emerald-500/8",
        activation.kind === "partial" && "border-amber-500/30 bg-amber-500/8",
        activation.kind === "inactive" && "bg-muted/30"
      )}
    >
      <span className="min-w-0 flex-1">
        <span className="block text-xs font-semibold">
          {t(`detail.activation.${activation.kind}`)}
        </span>
        <span className="mt-0.5 block text-[10px] leading-4 text-muted-foreground">
          {interactive
            ? t("detail.activation.count", {
                enabled: activation.enabledAgentCount,
                total: activation.agentCount,
              })
            : t(`detail.activation.${activation.kind}Hint`)}
        </span>
      </span>
      {busy || activation.kind === "loading" ? (
        <Loader2 className="size-4 shrink-0 animate-spin" />
      ) : interactive ? (
        <Switch
          checked={checked}
          disabled={disabled}
          aria-label={t("detail.activation.toggle")}
          onCheckedChange={onToggle}
        />
      ) : null}
    </section>
  )
}

function ManageMenu({
  detail,
  busy,
  onEdit,
  onDelete,
}: {
  detail: SkillMarketV2Detail
  busy: boolean
  onEdit: () => void
  onDelete: () => void
}) {
  const t = useTranslations("SkillMarketV2")
  if (!detail.canManage) return null
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="icon-sm"
          variant="outline"
          disabled={busy}
          aria-label={t("manage.more")}
          title={t("manage.more")}
        >
          <MoreHorizontal className="size-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem disabled={busy} onSelect={onEdit}>
          <Pencil className="size-3.5" />
          {t("manage.editMetadata")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          variant="destructive"
          disabled={busy}
          onSelect={onDelete}
        >
          <Trash2 className="size-3.5" />
          {t("manage.delete")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function DetailHeader(props: DetailHeaderProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const versions = props.versions.some(
    (item) => item.version === props.activeVersion.version
  )
    ? props.versions
    : [props.activeVersion, ...props.versions]
  const badges = [
    installStateBadgeInfo(props.detail.installState),
    audienceBadgeInfo(props.detail.audience),
    ...(props.detail.compatibility === "compatible"
      ? []
      : [compatibilityBadgeInfo(props.detail.compatibility)]),
  ]
  return (
    <header className="shrink-0 bg-background px-5 pt-5 pr-14 sm:px-6 sm:pt-6 sm:pr-16">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex min-w-0 gap-3.5">
          <Avatar className="size-12 shrink-0 rounded-md border bg-muted/30">
            {props.detail.iconUrl ? (
              <AvatarImage
                className="rounded-md"
                src={props.detail.iconUrl}
                alt=""
              />
            ) : null}
            <AvatarFallback className="rounded-md">
              <Package className="size-4" aria-hidden="true" />
            </AvatarFallback>
          </Avatar>
          <div className="min-w-0">
            <p className="truncate text-[10px] text-muted-foreground">
              {props.detail.organizationName ?? props.detail.category}
            </p>
            <h2 className="mt-0.5 truncate text-lg font-semibold">
              {props.detail.displayName}
              <span className="ml-2 font-mono text-[10px] font-normal text-muted-foreground">
                {props.detail.slug}
              </span>
            </h2>
            <p className="mt-1.5 line-clamp-2 max-w-2xl text-xs leading-5 text-muted-foreground">
              {props.detail.summary}
            </p>
          </div>
        </div>
        <ActivationControl
          activation={props.activation}
          busy={props.activationBusy}
          onToggle={props.onToggleActivation}
        />
      </div>
      <div className="mt-4 flex min-w-0 flex-col gap-3 border-t py-3 sm:flex-row sm:items-center">
        <MarketBadgeGroup
          badges={badges}
          limit={3}
          className="min-w-0 flex-1"
        />
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <Select
            value={props.activeVersion.version}
            onValueChange={props.onSelectVersion}
            disabled={props.versionsLoading}
          >
            <SelectTrigger className="w-40 rounded-md text-xs">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {versions.map((item) => (
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
            className="min-w-24"
            disabled={props.primaryDisabled}
            onClick={props.onPrimaryAction}
          >
            {t(`list.primary.${props.primaryKey}`)}
          </Button>
          <ManageMenu
            detail={props.detail}
            busy={props.activationBusy}
            onEdit={props.onEditMetadata}
            onDelete={props.onDelete}
          />
        </div>
      </div>
    </header>
  )
}
