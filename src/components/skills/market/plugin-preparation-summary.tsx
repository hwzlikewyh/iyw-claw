"use client"

import { useTranslations } from "next-intl"
import { pluginPreparationIssueKey } from "@/lib/plugin-preparation-copy"
import type { PluginPreparationController } from "./use-plugin-preparation"
import { PluginDependencyRows } from "./plugin-preparation-details"

interface SummaryProps {
  state: PluginPreparationController
  contextError: string
}

export function PreparationSummary({ state, contextError }: SummaryProps) {
  const t = useTranslations("PluginPreparation")
  return (
    <>
      <p role="status" className="text-sm">
        {t(
          state.error
            ? "unavailable"
            : state.busy
              ? "working"
              : state.connection
                ? "connectionVerified"
                : state.view
                  ? `state.${state.view.state}`
                  : "loading"
        )}
      </p>
      {state.view?.workspaceName ? (
        <>
          <p className="text-[13px] text-muted-foreground">
            {t("workspace", { name: state.view.workspaceName })}
          </p>
          <PluginDependencyRows view={state.view} />
        </>
      ) : null}
      {state.view?.issues.length ? (
        <ul className="space-y-1 text-[13px] text-destructive">
          {state.view.issues.map((issue) => (
            <li key={issue}>{t(pluginPreparationIssueKey(issue))}</li>
          ))}
        </ul>
      ) : null}
      {state.connection ? (
        <p className="text-[13px] text-muted-foreground">
          {t("verifiedTools", { count: state.connection.toolCount })}{" "}
          {t("accountHint")}
        </p>
      ) : null}
      {state.error || contextError ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error || contextError}
        </p>
      ) : null}
    </>
  )
}
