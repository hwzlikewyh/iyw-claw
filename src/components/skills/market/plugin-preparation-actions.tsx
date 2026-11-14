"use client"

import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { PluginPreparationController } from "./use-plugin-preparation"

interface ActionProps {
  state: PluginPreparationController
  contextBusy: boolean
  onRepair: () => void
  onOpenConnectors: () => void
}
interface Action {
  label: "refresh" | "check" | "manageConnectors" | "repair"
  disabled: boolean
  primary?: boolean
  run: () => void
}

export function PreparationActions(props: ActionProps) {
  const t = useTranslations("PluginPreparation")
  return (
    <div className="flex flex-wrap gap-2 [&_button]:text-[13px]">
      {actions(props).map((action) => (
        <Button
          key={action.label}
          size="sm"
          variant={action.primary ? "default" : "outline"}
          disabled={action.disabled}
          onClick={action.run}
        >
          {t(action.label)}
        </Button>
      ))}
    </div>
  )
}

function actions(props: ActionProps): Action[] {
  const { view, busy, error } = props.state
  const disabled = busy || props.contextBusy || !!error
  const result: Action[] = [
    { label: "refresh", disabled: busy, run: props.state.refresh },
  ]
  if (view?.state === "unchecked")
    result.push({
      label: "check",
      disabled,
      primary: true,
      run: () => void props.state.mutate(view, null),
    })
  if (view?.state === "legacy")
    result.push({
      label: "manageConnectors",
      disabled: busy,
      run: props.onOpenConnectors,
    })
  if (needsRepair(view, error))
    result.push({ label: "repair", disabled: busy, run: props.onRepair })
  return result
}

function needsRepair(view: PluginPreparationController["view"], error: string) {
  return (
    (!view && !!error) ||
    (view?.state === "blocked" &&
      view.issues.some((issue) =>
        [
          "dependency_snapshot_missing",
          "plugin_unavailable",
          "dependency_unavailable",
        ].includes(issue)
      ))
  )
}
