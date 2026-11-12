import { useEffect, useState } from "react"
import { getTransport } from "@/lib/transport"
import type { PluginPanelTab } from "./use-plugin-panels"

const KEEP_ALIVE_MS = 4 * 60 * 1000

export function usePluginPanelAuthority(tab: PluginPanelTab) {
  const [revoked, setRevoked] = useState(false)
  useEffect(() => {
    if (revoked) return
    let alive = true
    let dispose: (() => void) | undefined
    const transport = getTransport()
    const revoke = () => {
      if (alive) setRevoked(true)
    }
    const reconnect = transport.onReconnect?.(revoke)
    const stopKeepAlive = keepAlive(tab, transport, revoke)
    void transport
      .subscribe<{ instanceIds: string[] }>(
        "plugin-app://revoked",
        (payload) => {
          if (payload.instanceIds.includes(tab.id)) revoke()
        }
      )
      .then((value) => {
        if (alive) dispose = value
        else value()
      })
      .catch(revoke)
    return () => {
      alive = false
      stopKeepAlive()
      dispose?.()
      reconnect?.()
    }
  }, [tab, revoked])
  return revoked
}

function keepAlive(
  tab: PluginPanelTab,
  transport: ReturnType<typeof getTransport>,
  revoke: () => void
) {
  let pending = false
  const timer = setInterval(() => {
    if (pending) return
    pending = true
    void transport
      .call<{ accepted: boolean }>("plugin_app_message", {
        request: {
          instanceId: tab.id,
          leaseToken: tab.response.launch.leaseToken,
          nonce: tab.response.launch.nonce,
          method: "ping",
          params: {},
        },
      })
      .then((reply) => {
        if (!reply.accepted) revoke()
      })
      .catch(revoke)
      .finally(() => {
        pending = false
      })
  }, KEEP_ALIVE_MS)
  return () => clearInterval(timer)
}
