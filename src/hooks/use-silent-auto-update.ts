import { useEffect, useState } from "react"
import { getShellTransport } from "@/lib/transport"
import {
  getAppUpdatePreferences,
  usesTauriUpdater,
  type AppUpdatePreferences,
} from "@/lib/updater"

/** 未完成加载时也不弹更新提示；远程与服务器仍沿用原提示行为。 */
export function useSilentAutoUpdate() {
  const [silent, setSilent] = useState(usesTauriUpdater)
  useEffect(() => {
    if (!usesTauriUpdater()) return
    let cancelled = false
    let unlisten: (() => void) | null = null
    let changed = false
    const apply = (preferences: AppUpdatePreferences) => {
      if (!cancelled) setSilent(preferences.silentAutoUpdate)
    }
    void getShellTransport()
      .subscribe<AppUpdatePreferences>(
        "app_update_preferences",
        (preferences) => {
          changed = true
          apply(preferences)
        }
      )
      .then(async (off) => {
        if (cancelled) return off()
        unlisten = off
        const preferences = await getAppUpdatePreferences()
        if (!changed) apply(preferences)
      })
      .catch((error) =>
        console.warn("[Update] preference subscription failed:", error)
      )
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])
  return silent
}
