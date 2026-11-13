import { useCallback, useEffect, useState } from "react"
import { EmptyResultSchema } from "@modelcontextprotocol/sdk/types.js"
import i18n from "@/i18n"
import { useAgentStore } from "@/stores/use-agent-store"
import type {} from "../../../../../widget/src/upstream/host-bridge"

const CONNECTION_CHECK_MS = 30_000
const CONNECTION_TIMEOUT_MS = 5_000

export function useHostAgentConnection() {
  const [attempt, setAttempt] = useState(0)
  const reconnect = useCallback(() => setAttempt((value) => value + 1), [])
  useEffect(() => {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const update = useAgentStore.getState().setAgentState
    const check = async () => {
      const app = window.__IYW_CANVAS_BRIDGE__?.app
      try {
        if (!app?.getHostCapabilities()?.message?.text) throw new Error(i18n.t("hostAgent.unsupported"))
        await app.request({ method: "ping" }, EmptyResultSchema, { timeout: CONNECTION_TIMEOUT_MS })
        if (!stopped) update({ connected: true, enabled: true, connectError: "" })
      } catch (error) {
        if (!stopped) update({ connected: false, enabled: false, connectError: error instanceof Error ? error.message : i18n.t("hostAgent.unavailable") })
      } finally { if (!stopped) timer = setTimeout(() => void check(), CONNECTION_CHECK_MS) }
    }
    update({ connected: false, enabled: true, connectError: "", activeTab: "chat" })
    void check()
    return () => { stopped = true; if (timer) clearTimeout(timer) }
  }, [attempt])
  return reconnect
}
