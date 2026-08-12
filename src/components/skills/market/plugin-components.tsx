"use client"

import { Link2, PlugZap, Sparkles } from "lucide-react"
import { useTranslations } from "next-intl"
import type {
  SkillPluginBinding,
  SkillPluginComponent,
  SkillPluginManifest,
} from "@/lib/skill-market"

export function PluginComponents({
  plugin,
}: {
  plugin: SkillPluginManifest | null | undefined
}) {
  const t = useTranslations("SkillMarketV2")
  if (!plugin?.components.length) {
    return (
      <p className="text-xs text-muted-foreground">
        {t("detail.noPluginComponents")}
      </p>
    )
  }

  return (
    <div className="space-y-4">
      <PluginComponentList components={plugin.components} />
      <PluginBindings bindings={plugin.bindings} />
    </div>
  )
}

function PluginComponentList({
  components,
}: {
  components: SkillPluginComponent[]
}) {
  return (
    <div className="space-y-2">
      {components.map((component) => (
        <PluginComponentRow
          key={`${component.type}:${component.key}`}
          component={component}
        />
      ))}
    </div>
  )
}

function PluginComponentRow({
  component,
}: {
  component: SkillPluginComponent
}) {
  const t = useTranslations("SkillMarketV2")
  const Icon = component.type === "skill" ? Sparkles : PlugZap
  const detail =
    component.type === "skill"
      ? component.path || "-"
      : component.serverKey || "-"
  const detailLabel =
    component.type === "skill"
      ? t("detail.componentPath")
      : t("detail.componentServer")
  return (
    <div className="flex min-w-0 items-start gap-3 border bg-background px-3 py-2">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-xs font-medium">{component.key}</span>
          <span className="shrink-0 text-[10px] text-muted-foreground">
            {t(`detail.componentType.${component.type}`)}
          </span>
        </div>
        <p className="mt-1 break-all font-mono text-[10px] text-muted-foreground">
          {detailLabel}: {detail}
        </p>
      </div>
    </div>
  )
}

function PluginBindings({ bindings }: { bindings: SkillPluginBinding[] }) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="space-y-2">
      <h3 className="text-xs font-medium">{t("detail.bindings")}</h3>
      {bindings.length ? (
        bindings.map((binding) => (
          <PluginBindingRow
            key={`${binding.skillKey}:${binding.connectorKey}`}
            binding={binding}
          />
        ))
      ) : (
        <p className="text-xs text-muted-foreground">
          {t("detail.noBindings")}
        </p>
      )}
    </div>
  )
}

function PluginBindingRow({ binding }: { binding: SkillPluginBinding }) {
  return (
    <div className="flex min-w-0 items-center gap-2 border-b py-2 text-xs last:border-b-0">
      <span className="min-w-0 flex-1 truncate font-mono">
        {binding.skillKey}
      </span>
      <Link2 className="size-3.5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1 truncate text-right font-mono">
        {binding.connectorKey}
      </span>
    </div>
  )
}
