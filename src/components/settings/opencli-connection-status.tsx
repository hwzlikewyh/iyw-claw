import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import type { OpencliSetup } from "./use-opencli-setup"

const STATUS_KEYS = [
  "status.loading",
  "status.installing",
  "status.preparing",
  "status.checking",
  "status.uninstalling",
  "status.installed",
  "status.update_available",
  "status.not_runnable",
  "status.not_installed",
  "status.connected",
  "status.daemon_not_running",
  "status.extension_disconnected",
  "status.profile_selection_required",
  "status.selected_profile_disconnected",
  "status.connectivity_failed",
  "status.unrecognized_doctor_report",
  "status.runtime_error",
] as const

export function OpencliConnectionStatus({ setup }: { setup: OpencliSetup }) {
  const t = useTranslations("BrowserSetup")
  const status =
    setup.action ??
    (setup.error ? "runtime_error" : null) ??
    setup.doctor?.status ??
    setup.tool?.status ??
    "not_installed"
  const key =
    STATUS_KEYS.find((item) => item === `status.${status}`) ??
    "status.runtime_error"
  const label = t(key)
  return (
    <p
      role="status"
      className={`mt-1 flex items-center gap-1.5 text-xs ${setup.doctor?.ok && !setup.action ? "text-green-600 dark:text-green-400" : "text-muted-foreground"}`}
    >
      {setup.action && <Loader2 className="size-3 shrink-0 animate-spin" />}
      <span>OpenCLI · {label}</span>
    </p>
  )
}
