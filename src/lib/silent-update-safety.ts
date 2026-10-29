import { useEffect } from "react"
import {
  isDesktop,
  getShellTransport,
  isRemoteDesktopMode,
} from "@/lib/transport"

const CHANGE_EVENT = "app:silent-update-safety"
let filesBlocked: boolean | null = null

function listenForSafetyChanges(notify: () => void) {
  const events = [CHANGE_EVENT, "blur", "focus"]
  for (const event of events) window.addEventListener(event, notify)
  document.addEventListener("visibilitychange", notify)
  return () => {
    for (const event of events) window.removeEventListener(event, notify)
    document.removeEventListener("visibilitychange", notify)
  }
}

function createSafetyReporter(pathname: string, isCancelled: () => boolean) {
  const transport = getShellTransport()
  let queue = Promise.resolve()
  return (generation: number) => {
    // 串行上报，防止较旧的“安全”快照覆盖更新的未保存状态。
    queue = queue
      .then(async () => {
        if (isCancelled()) return
        const blocked =
          isRemoteDesktopMode() ||
          (pathname.startsWith("/workspace")
            ? filesBlocked !== false
            : !pathname.startsWith("/settings"))
        await transport.call("report_app_update_safety", {
          generation,
          blocked,
        })
      })
      .catch((error) => console.warn("[Update] safety report failed:", error))
  }
}

/** 文件状态未知也阻止安装；卸载工作区不能被当作已经保存。 */
export function useWorkspaceUpdateBlocker(blocked: boolean) {
  useEffect(() => {
    filesBlocked = blocked
    window.dispatchEvent(new Event(CHANGE_EVENT))
    return () => {
      filesBlocked = null
      window.dispatchEvent(new Event(CHANGE_EVENT))
    }
  }, [blocked])
}

export function useSilentUpdateSafety(pathname: string) {
  useEffect(() => {
    if (!isDesktop()) return
    let cancelled = false
    const report = createSafetyReporter(pathname, () => cancelled)
    let unlisten: (() => void) | null = null
    void getShellTransport()
      .call<number>("report_app_update_safety", {
        generation: null,
        blocked: true,
      })
      .then((generation) => {
        if (cancelled) return
        const notify = () => report(generation)
        unlisten = listenForSafetyChanges(notify)
        notify()
      })
      .catch((error) =>
        console.warn("[Update] safety initialization failed:", error)
      )
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [pathname])
}
