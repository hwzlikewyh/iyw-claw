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
  chromix: "Chromix",
  "agent-browser": "agent-browser",
  officecli: "OfficeCLI",
  "agent-reach": "Agent Reach",
}

export function runtimeComponentLabel(
  id: string,
  labels: { computer: string; environment: string; agent: string }
) {
  if (id === "cua-driver") return labels.computer
  if (id === "environment") return labels.environment
  if (id === "builtin-agent") return labels.agent
  return COMPONENT_NAMES[id] ?? id
}

export function RuntimeComponentStatus({
  components,
}: {
  components: BootstrapComponentStatus[]
}) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const labels = {
    computer: t("computer"),
    environment: t("title"),
    agent: t("builtinAgent"),
  }
  return (
    <SettingSection title={t("componentsTitle")}>
      {components.length === 0 ? (
        <p className="px-4 py-3 text-sm text-muted-foreground">{t("empty")}</p>
      ) : (
        components.map((component) => {
          const ready = component.installed && component.active
          const Icon = ready ? CheckCircle2 : CircleAlert
          return (
            <SettingRow
              key={component.componentId}
              title={runtimeComponentLabel(component.componentId, labels)}
              description={[component.version, component.lastError]
                .filter(Boolean)
                .join(" · ")}
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
  const labels = {
    computer: t("computer"),
    environment: t("title"),
    agent: t("builtinAgent"),
  }
  const percent =
    typeof progress?.percent === "number" && Number.isFinite(progress.percent)
      ? Math.max(0, Math.min(100, progress.percent))
      : null
  return (
    <div className="grid gap-2" role="status" aria-live="polite">
      <p className="text-sm text-muted-foreground">
        {progress?.component
          ? `${runtimeComponentLabel(progress.component, labels)} · `
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
