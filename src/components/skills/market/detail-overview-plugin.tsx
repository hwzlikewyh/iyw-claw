"use client"

import { useTranslations } from "next-intl"
import type {
  SkillMarketTranslator,
  SkillMarketV2Version,
} from "@/lib/skill-market"

const HOST_ACTION_KEYS = {
  "send-message": "sendMessage",
  "clipboard-write": "clipboardWrite",
  "open-link": "openLink",
  camera: "camera",
  microphone: "microphone",
  geolocation: "geolocation",
} as const

interface PermissionRow {
  key: string
  values: string[]
}

function permissionRows(version: SkillMarketV2Version): PermissionRow[] {
  const permissions = version.plugin?.permissions
  if (!permissions) return []
  return [
    { key: "read", values: permissions.workspace.read },
    { key: "write", values: permissions.workspace.write },
    { key: "network", values: permissions.network.connectDomains },
    { key: "resources", values: permissions.network.resourceDomains },
    { key: "frames", values: permissions.network.frameDomains },
    { key: "baseUris", values: permissions.network.baseUriDomains ?? [] },
  ].filter((item) => item.values.length > 0)
}

function PermissionRows({ rows }: { rows: PermissionRow[] }) {
  const t = useTranslations(
    "PluginPreparation.permissions"
  ) as unknown as SkillMarketTranslator
  return rows.map((row) => (
    <div
      key={row.key}
      className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-3 py-2 text-xs"
    >
      <dt className="text-muted-foreground">{t(row.key)}</dt>
      <dd className="break-words">{row.values.join(" · ")}</dd>
    </div>
  ))
}

export function PluginComponentSummary({
  version,
}: {
  version: SkillMarketV2Version
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  if (!version.plugin)
    return (
      <p className="text-xs text-muted-foreground">
        {t("detail.noPluginManifest")}
      </p>
    )
  if (!version.plugin.components.length)
    return (
      <p className="text-xs text-muted-foreground">
        {t("detail.noPluginComponents")}
      </p>
    )
  const counts = new Map<string, number>()
  for (const component of version.plugin.components)
    counts.set(component.type, (counts.get(component.type) ?? 0) + 1)
  return (
    <div className="flex flex-wrap gap-2">
      {[...counts].map(([type, count]) => (
        <span
          key={type}
          className="rounded-md border bg-muted/30 px-2 py-1 text-xs"
        >
          {t(`detail.componentType.${type}`)} · {count}
        </span>
      ))}
    </div>
  )
}

function EmptyPermissions({ message }: { message: string }) {
  return <p className="text-xs text-muted-foreground">{message}</p>
}

export function PluginPermissionSummary({
  version,
}: {
  version: SkillMarketV2Version
}) {
  const t = useTranslations(
    "PluginPreparation.permissions"
  ) as unknown as SkillMarketTranslator
  const market = useTranslations("SkillMarketV2")
  const host = useTranslations("PluginPreparation.hostActions")
  const permissions = version.plugin?.permissions
  if (!version.plugin)
    return <EmptyPermissions message={market("detail.noPluginManifest")} />
  if (!permissions)
    return <EmptyPermissions message={market("detail.permissionsUndeclared")} />
  const rows = permissionRows(version)
  const hostActions = permissions.host.map((action) =>
    host(HOST_ACTION_KEYS[action])
  )
  if (!rows.length && !hostActions.length)
    return <EmptyPermissions message={market("detail.noPermissions")} />
  return (
    <dl className="divide-y border-y">
      <PermissionRows rows={rows} />
      {hostActions.length > 0 && (
        <div className="grid grid-cols-[6.5rem_minmax(0,1fr)] gap-3 py-2 text-xs">
          <dt className="text-muted-foreground">{t("host")}</dt>
          <dd className="break-words">{hostActions.join(" · ")}</dd>
        </div>
      )}
    </dl>
  )
}
