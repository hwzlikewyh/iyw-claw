"use client"

import { useTranslations } from "next-intl"

export function unavailableAutomationModel(error?: string | null) {
  return error?.match(
    /Selected model '([^'\r\n]+)' is unavailable or was not confirmed by the Agent\./i
  )?.[1]
}

export function useAutomationErrorMessage() {
  const t = useTranslations("Automations")
  return (error: unknown) => {
    const message = error instanceof Error ? error.message : String(error)
    const model = unavailableAutomationModel(message)
    if (model) return t("modelUnavailable", { model })
    if (message.includes("agent process exited unexpectedly"))
      return t("agentStartFailed")
    return message
  }
}
