"use client"

import { useTranslations } from "next-intl"
import { Badge } from "@/components/ui/badge"
import type { LocalMcpServer } from "@/lib/types"

export function pluginSources(server: LocalMcpServer) {
  return (server.sources ?? []).filter((source) => source.kind === "plugin")
}

export function connectorName(server: LocalMcpServer): string {
  return server.display_name?.trim() || server.id
}

export function ConnectorListMetadata({ server }: { server: LocalMcpServer }) {
  const t = useTranslations("McpSettings")
  const sources = pluginSources(server)
  const needsConfig = (server.missing_config ?? []).length > 0

  return (
    <div className="mt-1 flex flex-wrap gap-1">
      <Badge variant="outline" className="text-[9px]">
        {t("local.global")}
      </Badge>
      {needsConfig ? (
        <Badge variant="secondary" className="text-[9px]">
          {t("local.needsConfig")}
        </Badge>
      ) : null}
      {sources.slice(0, 1).map((source) => (
        <Badge
          key={`${source.ownerId}:${source.componentKey}`}
          variant="outline"
          className="max-w-36 truncate text-[9px]"
        >
          {t("local.fromPlugin", { name: source.ownerName })}
        </Badge>
      ))}
    </div>
  )
}

export function ConnectorDetailMetadata({
  server,
}: {
  server: LocalMcpServer
}) {
  const t = useTranslations("McpSettings")
  const sources = pluginSources(server)
  const skillKeys = Array.from(
    new Set(sources.flatMap((source) => source.requiredSkillKeys))
  )
  const missingConfig = server.missing_config ?? []

  return (
    <>
      <ConnectorSourceBadges server={server} />
      {skillKeys.length > 0 ? (
        <RequiredSkillBadges skillKeys={skillKeys} />
      ) : null}
      {missingConfig.length > 0 ? (
        <div className="border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-700 dark:text-amber-300">
          {t("local.missingConfig", { fields: missingConfig.join(", ") })}
        </div>
      ) : null}
    </>
  )
}

function ConnectorSourceBadges({ server }: { server: LocalMcpServer }) {
  const t = useTranslations("McpSettings")
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <Badge variant={server.enabled ? "default" : "secondary"}>
        {server.enabled
          ? t("local.enabledGlobally")
          : t("local.disabledGlobally")}
      </Badge>
      {pluginSources(server).map((source) => (
        <Badge
          key={`${source.ownerId}:${source.componentKey}`}
          variant="outline"
        >
          {t("local.pluginSourceVersion", {
            name: source.ownerName,
            version: source.version,
          })}
        </Badge>
      ))}
    </div>
  )
}

function RequiredSkillBadges({ skillKeys }: { skillKeys: string[] }) {
  const t = useTranslations("McpSettings")
  return (
    <div className="space-y-1.5">
      <div className="text-xs text-muted-foreground">
        {t("local.usedBySkills")}
      </div>
      <div className="flex flex-wrap gap-1.5">
        {skillKeys.map((skillKey) => (
          <Badge key={skillKey} variant="secondary">
            {skillKey}
          </Badge>
        ))}
      </div>
    </div>
  )
}
