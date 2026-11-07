"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { RecoveryActions } from "./plugin-recovery-actions"
import { usePluginRecovery } from "./use-plugin-recovery"
import type { PluginRecoveryReview } from "@/lib/plugin-recovery"

interface Props {
  id: string
  onRecovered: () => void
  onClose: () => void
  onReinstall: () => void
}

export function PluginRecoveryDialog(props: Props) {
  const t = useTranslations("PluginRecovery"),
    state = usePluginRecovery(props.id, props.onRecovered)
  const [inspected, setInspected] = useState(false)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.busy) props.onClose()
      }}
    >
      <DialogContent
        showCloseButton={!state.busy}
        className="flex max-h-[85dvh] flex-col overflow-hidden"
      >
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription>{t("hint")}</DialogDescription>
        </DialogHeader>
        <RecoveryBody
          state={state}
          inspected={inspected}
          onClose={props.onClose}
        />
        <RecoveryActions
          {...props}
          state={state}
          inspected={inspected}
          onInspect={() => {
            setInspected(true)
            void state.inspect()
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

function RecoveryDetails({
  view,
  onNavigate,
  busy,
}: {
  view: PluginRecoveryReview
  onNavigate: () => void
  busy: boolean
}) {
  const t = useTranslations("PluginRecovery")
  return (
    <div className="space-y-3 text-sm">
      <p className="break-words font-medium">
        {view.slug} · {view.version}
      </p>
      {view.listed === false ? <p>{t("unlisted")}</p> : null}
      <p>{t(view.scopeChanged ? "scopeChanged" : "sameScope")}</p>
      <p>{t(view.resetPermissions ? "resetPermissions" : "keepPermissions")}</p>
      <p className="text-muted-foreground">{t("keepFiles")}</p>
      {view.issues.length ? (
        <ul className="space-y-1 text-destructive">
          {view.issues.map((issue, index) => (
            <li key={index} className="break-words">
              {issue}
            </li>
          ))}
        </ul>
      ) : null}
      <RecoveryDependencies view={view} onNavigate={onNavigate} busy={busy} />
    </div>
  )
}

function RecoveryDependencies({
  view,
  onNavigate,
  busy,
}: {
  view: PluginRecoveryReview
  onNavigate: () => void
  busy: boolean
}) {
  const t = useTranslations("PluginRecovery"),
    route = useWorkbenchRoute()
  return (
    <>
      {view.dependencies.length ? (
        <ul className="space-y-2">
          {view.dependencies.map((dependency) => (
            <li
              key={dependency.id}
              className="flex flex-wrap items-start justify-between gap-2"
            >
              <Button
                size="xs"
                variant="link"
                disabled={busy}
                className="h-auto whitespace-normal p-0 text-left text-[13px]"
                onClick={() => {
                  onNavigate()
                  route.openSkillMarket(dependency.slug, dependency.packageType)
                }}
              >
                {dependency.slug} · {dependency.requiredVersion}
              </Button>
              <span
                className={
                  dependency.ready
                    ? "text-muted-foreground"
                    : "text-destructive"
                }
              >
                {t(dependency.ready ? "dependencyReady" : "dependencyBlocked")}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </>
  )
}

function RecoveryBody({
  state,
  inspected,
  onClose,
}: {
  state: ReturnType<typeof usePluginRecovery>
  inspected: boolean
  onClose: () => void
}) {
  const t = useTranslations("PluginRecovery")
  return (
    <div className="min-h-0 space-y-4 overflow-y-auto">
      <p role="status" className="text-sm">
        {t(
          state.busy
            ? "working"
            : state.view
              ? `state.${state.view.state}`
              : inspected
                ? "inspectFailed"
                : "notInspected"
        )}
      </p>
      {state.view ? (
        <RecoveryDetails
          view={state.view}
          onNavigate={onClose}
          busy={state.busy}
        />
      ) : null}
      {state.error ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
    </div>
  )
}
