"use client"

import { CheckCircle2, CircleAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import { Progress } from "@/components/ui/progress"
import { SettingRow, SettingSection } from "@/components/settings/settings-ui"
import type { BootstrapComponentStatus, BootstrapInitEvent } from "@/lib/types"

const COMPONENT_NAMES: Record<string, string> = {
  node: "Node.js",
  git: "Git",
  uv: "uv",
}

export function runtimeComponentLabel(
  id: string,
  basicComponentsLabel: string
) {
  return COMPONENT_NAMES[id] ?? basicComponentsLabel
}

function runtimeComponentRows(components: BootstrapComponentStatus[]) {
  const toolIds = Object.keys(COMPONENT_NAMES)
  const tools = toolIds.flatMap((id) => {
    const component = components.find((item) => item.componentId === id)
    return component
      ? [
          {
            componentId: id,
            ready: component.installed && component.active,
            description: [component.version, component.lastError]
              .filter(Boolean)
              .join(" · "),
          },
        ]
      : []
  })
  const basicComponents = components.filter(
    (component) => !toolIds.includes(component.componentId)
  )
  if (basicComponents.length === 0) return tools
  return [
    ...tools,
    {
      componentId: "basic-components",
      ready: basicComponents.every(
        (component) => component.installed && component.active
      ),
      description: basicComponents
        .filter((component) => !component.installed || !component.active)
        .map((component) => component.lastError)
        .filter(Boolean)
        .join(" · "),
    },
  ]
}

export function RuntimeComponentStatus({
  components,
}: {
  components: BootstrapComponentStatus[]
}) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const rows = runtimeComponentRows(components)
  return (
    <SettingSection title={t("componentsTitle")}>
      {components.length === 0 ? (
        <p className="px-4 py-3 text-sm text-muted-foreground">{t("empty")}</p>
      ) : (
        rows.map((component) => {
          const ready = component.ready
          const Icon = ready ? CheckCircle2 : CircleAlert
          return (
            <SettingRow
              key={component.componentId}
              title={runtimeComponentLabel(
                component.componentId,
                t("basicComponents")
              )}
              description={component.description}
            >
              <span
                className={`flex items-center gap-1.5 text-xs ${ready ? "text-green-600" : "text-amber-600"}`}
              >
                <Icon className="size-4" aria-hidden="true" />
                {ready ? t("ready") : t("needsRepair")}
              </span>
            </SettingRow>
          )
        })
      )}
    </SettingSection>
  )
}

export function RuntimeRepairProgress({
  progress,
}: {
  progress: BootstrapInitEvent | null
}) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const percent =
    typeof progress?.percent === "number" && Number.isFinite(progress.percent)
      ? Math.max(0, Math.min(100, progress.percent))
      : null
  return (
    <div className="grid gap-2" role="status" aria-live="polite">
      <p className="text-sm text-muted-foreground">
        {progress?.component
          ? `${runtimeComponentLabel(progress.component, t("basicComponents"))} · `
          : ""}
        {progress && t.has(`phases.${progress.phase}`)
          ? t(`phases.${progress.phase}`)
          : t("repairing")}
      </p>
      {percent !== null && <Progress value={percent} className="h-1.5" />}
      {progress?.message && (
        <p className="text-xs break-words text-muted-foreground">
          {progress.message}
        </p>
      )}
    </div>
  )
}
