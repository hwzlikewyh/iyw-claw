"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { runtimeComponentLabel } from "@/components/settings/runtime-environment-status"
import { openSettingsWindow } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { useRuntimeEnvironment } from "@/lib/runtime-environment"

const ENVIRONMENT_TOAST_ID = "desktop-runtime-environment"

async function openRuntimeSettings() {
  try {
    await openSettingsWindow("runtime-environment")
  } catch (error) {
    console.warn("[DesktopRuntimeMonitor] Failed to open settings:", error)
    toast.error(toErrorMessage(error))
  }
}

/** 环境异常只提示，显式修复继续使用设置页的既有流程。 */
export function DesktopRuntimeMonitor() {
  const t = useTranslations("RuntimeEnvironmentSettings")
  const { report, error, busy } = useRuntimeEnvironment()

  useEffect(() => {
    if (busy) return
    if (!error && (!report || report.phase === "ready")) {
      toast.dismiss(ENVIRONMENT_TOAST_ID)
      return
    }
    const unavailable =
      report?.components.filter((item) => !item.installed || !item.active) ?? []
    const labels = {
      computer: t("computer"),
      environment: t("title"),
      agent: t("builtinAgent"),
    }
    const details = unavailable
      .map(
        (item) =>
          `${runtimeComponentLabel(item.componentId, labels)}: ${item.lastError ?? t("needsRepair")}`
      )
      .join("\n")
    console.warn("[DesktopRuntimeMonitor] Environment needs attention:", {
      phase: report?.phase,
      components: unavailable.map((item) => ({
        id: item.componentId,
        reason: item.lastError,
      })),
      error,
    })
    toast.warning(error ? t("unknown") : t("degraded"), {
      id: ENVIRONMENT_TOAST_ID,
      description: error || details || t("degraded"),
      classNames: {
        description: "max-h-32 overflow-y-auto break-words whitespace-pre-wrap",
      },
      duration: Infinity,
      closeButton: true,
      action: { label: t("repair"), onClick: () => void openRuntimeSettings() },
    })
    return () => {
      toast.dismiss(ENVIRONMENT_TOAST_ID)
    }
  }, [report, error, busy, t])

  return null
}
