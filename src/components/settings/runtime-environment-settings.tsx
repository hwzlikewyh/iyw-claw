"use client"

import { Loader2, RefreshCw, Wrench } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  SettingsPageHeader,
  SettingsPageLayout,
  SettingSection,
  SettingSectionBody,
} from "@/components/settings/settings-ui"
import {
  RuntimeComponentStatus,
  RuntimeRepairProgress,
} from "./runtime-environment-status"
import { useRuntimeEnvironment } from "@/lib/runtime-environment"

export function RuntimeEnvironmentSettings() {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const state = useRuntimeEnvironment()
  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Wrench}
        title={t("title")}
        description={t("description")}
        action={
          <Button
            variant="outline"
            size="sm"
            disabled={!!state.busy}
            onClick={() => void state.refresh()}
          >
            {state.busy === "checking" ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <RefreshCw className="size-4" />
            )}
            {t("check")}
          </Button>
        }
      />
      <RuntimeRepairSection state={state} />
      {state.report && (
        <RuntimeComponentStatus components={state.report.components} />
      )}
    </SettingsPageLayout>
  )
}

type EnvironmentState = ReturnType<typeof useRuntimeEnvironment>

function RuntimeRepairSection({ state }: { state: EnvironmentState }) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  return (
    <SettingSection
      title={t("repairTitle")}
      description={t("repairDescription")}
    >
      <SettingSectionBody>
        <div className="grid gap-3">
          <RuntimeEnvironmentSummary state={state} />
          {state.report?.writerBusy && (
            <p className="text-xs text-muted-foreground">{t("writerBusy")}</p>
          )}
          {state.report?.offline && (
            <p className="text-xs text-muted-foreground">{t("offline")}</p>
          )}
          <RuntimeRepairButton state={state} />
          {state.busy === "repairing" && (
            <RuntimeRepairProgress progress={state.progress} />
          )}
          {state.error && (
            <p role="alert" className="text-sm break-words text-destructive">
              {state.error}
            </p>
          )}
          {state.repaired && (
            <p role="status" className="text-sm text-green-600">
              {t("success")}
            </p>
          )}
        </div>
      </SettingSectionBody>
    </SettingSection>
  )
}

function RuntimeEnvironmentSummary({ state }: { state: EnvironmentState }) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const key =
    state.busy === "checking"
      ? "checking"
      : !state.report
        ? "unknown"
        : state.report.phase === "ready"
          ? "healthy"
          : "degraded"
  return (
    <p role="status" className="text-sm">
      {t(key)}
    </p>
  )
}

function RuntimeRepairButton({ state }: { state: EnvironmentState }) {
  const t = useTranslations("RuntimeEnvironmentSettings")
  return (
    <div>
      <Button
        disabled={!!state.busy || !!state.report?.writerBusy}
        onClick={() => void state.repair()}
      >
        {state.busy === "repairing" ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Wrench className="size-4" />
        )}
        {state.busy === "repairing" ? t("repairing") : t("repair")}
      </Button>
    </div>
  )
}
