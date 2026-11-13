import { useCallback, useEffect, useState } from "react"
import i18n from "@/i18n"
import { useAgentStore } from "@/stores/use-agent-store"
import { readCurrentWindow, type HostWindowContext } from "../../../../../widget/src/upstream/host-bridge"

const CONNECTION_CHECK_MS = 30_000

export function useHostAgentConnection() {
  const [attempt, setAttempt] = useState(0)
  const [currentWindow, setCurrentWindow] = useState<HostWindowContext>()
  const reconnect = useCallback(() => setAttempt((value) => value + 1), [])
  useEffect(() => {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const update = useAgentStore.getState().setAgentState
    const check = async () => {
      const app = window.__IYW_CANVAS_BRIDGE__?.app
      try {
        if (!app) throw new Error("hostAgent.unsupported")
        const context = await readCurrentWindow(app)
        if (!stopped) {
          window.__IYW_CANVAS_BRIDGE__!.currentWindow = context
          setCurrentWindow(context)
          update({ connected: true, enabled: true, connectError: "" })
        }
      } catch (error) {
        if (!stopped) { setCurrentWindow(undefined); update({ connected: false, enabled: false, connectError: i18n.t(error instanceof Error && error.message.startsWith("hostAgent.") ? error.message : "hostAgent.unavailable") }) }
      } finally { if (!stopped) timer = setTimeout(() => void check(), CONNECTION_CHECK_MS) }
    }
    update({ connected: false, enabled: true, connectError: "", activeTab: "chat" })
    void check()
    return () => { stopped = true; if (timer) clearTimeout(timer) }
  }, [attempt])
  useEffect(() => {
    const app = window.__IYW_CANVAS_BRIDGE__?.app
    const contextChanged = (context: { containerDimensions?: unknown }) => { if (!context.containerDimensions) reconnect() }
    app?.addEventListener("hostcontextchanged", contextChanged)
    return () => app?.removeEventListener("hostcontextchanged", contextChanged)
  }, [reconnect])
  return { reconnect, currentWindow }
}
