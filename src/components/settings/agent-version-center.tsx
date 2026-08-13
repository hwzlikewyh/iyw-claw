"use client"

import {
  Download,
  Loader2,
  Pin,
  PinOff,
  RefreshCw,
  RotateCcw,
  Shuffle,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import type { AgentType, AgentVersionInventory } from "@/lib/types"

import { useAgentVersionCenter } from "./use-agent-version-center"

interface AgentVersionCenterProps {
  agentType: AgentType
  onChanged: () => Promise<void>
}

export function AgentVersionCenter(props: AgentVersionCenterProps) {
  const state = useAgentVersionCenter(props)

  if (!state.ready || !state.inventory) {
    return (
      <section className="border-b pb-4">
        <div className="flex min-h-16 items-center justify-center text-xs text-muted-foreground">
          {state.error ? (
            state.error
          ) : (
            <Loader2 className="h-4 w-4 animate-spin" />
          )}
        </div>
      </section>
    )
  }

  return <VersionCenterContent state={state} inventory={state.inventory} />
}

function VersionCenterContent({
  state,
  inventory,
}: {
  state: ReturnType<typeof useAgentVersionCenter>
  inventory: AgentVersionInventory
}) {
  const t = useTranslations("AcpAgentSettings.versionCenter")
  return (
    <section className="space-y-3 border-b pb-4">
      <VersionCenterHeader
        busy={state.busy !== null}
        onRefresh={state.refresh}
      />
      <div className="grid gap-2 text-xs sm:grid-cols-2 xl:grid-cols-4">
        <VersionDatum label={t("active")} value={inventory.activeVersion} />
        <VersionDatum label={t("recommended")} value={state.recommended} />
        <VersionDatum label={t("pinned")} value={inventory.pinnedVersion} />
        <VersionDatum
          label={t("lastKnownGood")}
          value={inventory.lastKnownGoodVersion}
        />
      </div>
      <VersionActions state={state} />
      <div className="flex flex-wrap items-center gap-1.5">
        <Badge variant="outline">
          {t("channel", { value: inventory.updateChannel })}
        </Badge>
        <Badge variant="outline">
          {t("policy", { value: inventory.updatePolicy })}
        </Badge>
        {state.catalogStale ? (
          <Badge variant="destructive">{t("catalogStale")}</Badge>
        ) : null}
        {state.accessDenied ? (
          <Badge variant="destructive">{t("platformDisabled")}</Badge>
        ) : null}
      </div>
      {state.error ? (
        <p className="break-words text-xs text-destructive">{state.error}</p>
      ) : null}
    </section>
  )
}

function VersionCenterHeader({
  busy,
  onRefresh,
}: {
  busy: boolean
  onRefresh: () => void
}) {
  const t = useTranslations("AcpAgentSettings.versionCenter")
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0">
        <h4 className="text-sm font-medium">{t("title")}</h4>
        <p className="mt-1 text-xs text-muted-foreground">{t("description")}</p>
      </div>
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        title={t("refresh")}
        aria-label={t("refresh")}
        disabled={busy}
        onClick={onRefresh}
      >
        <RefreshCw className={busy ? "animate-spin" : ""} />
      </Button>
    </div>
  )
}

function VersionActions({
  state,
}: {
  state: ReturnType<typeof useAgentVersionCenter>
}) {
  const t = useTranslations("AcpAgentSettings.versionCenter")
  const disabled = state.busy !== null || state.accessDenied
  const pinDisabled =
    disabled || (!state.isPinned && (!state.canPin || !state.isActive))
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select value={state.selectedVersion} onValueChange={state.selectVersion}>
        <SelectTrigger size="sm" className="min-w-40 max-w-full rounded-md">
          <SelectValue placeholder={t("selectVersion")} />
        </SelectTrigger>
        <SelectContent>
          {state.versions.map((item) => (
            <SelectItem key={item.id} value={item.version}>
              {item.version}
              {item.recommended ? ` · ${t("recommendedBadge")}` : ""}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <PrimaryVersionAction state={state} disabled={disabled} />
      <Button
        size="sm"
        variant="ghost"
        disabled={pinDisabled}
        onClick={state.togglePin}
      >
        {state.isPinned ? <PinOff /> : <Pin />}
        {state.isPinned ? t("unpin") : t("pin")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        disabled={disabled || !state.rollbackReady}
        onClick={state.rollback}
      >
        {state.busy === "rollback" ? (
          <Loader2 className="animate-spin" />
        ) : (
          <RotateCcw />
        )}
        {t("rollback")}
      </Button>
    </div>
  )
}

function PrimaryVersionAction({
  state,
  disabled,
}: {
  state: ReturnType<typeof useAgentVersionCenter>
  disabled: boolean
}) {
  const t = useTranslations("AcpAgentSettings.versionCenter")
  if (!state.isInstalled) {
    return (
      <Button
        size="sm"
        variant="outline"
        disabled={disabled || !state.selectedVersion}
        onClick={state.install}
      >
        {state.busy === "install" ? (
          <Loader2 className="animate-spin" />
        ) : (
          <Download />
        )}
        {t("install")}
      </Button>
    )
  }
  if (state.isActive) return null
  return (
    <Button
      size="sm"
      variant="outline"
      disabled={disabled}
      onClick={state.switchVersion}
    >
      {state.busy === "switch" ? (
        <Loader2 className="animate-spin" />
      ) : (
        <Shuffle />
      )}
      {t("switch")}
    </Button>
  )
}

function VersionDatum({
  label,
  value,
}: {
  label: string
  value: string | null
}) {
  return (
    <div className="min-w-0">
      <p className="text-muted-foreground">{label}</p>
      <p className="mt-0.5 truncate font-mono font-medium">{value || "-"}</p>
    </div>
  )
}
