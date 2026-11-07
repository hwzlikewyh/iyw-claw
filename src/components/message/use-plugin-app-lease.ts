import { useEffect, useRef, useState } from "react"
import { getTransport } from "@/lib/transport"
import type { PluginAppLaunch } from "@/lib/plugin-app-bridge"

export function usePluginAppLease(launch: PluginAppLaunch) {
  const [transport] = useState(getTransport)
  const generation = useRef({ value: 0, instanceId: "" })
  useEffect(() => {
    const counter = generation.current
    const current = ++counter.value
    counter.instanceId = launch.instanceId
    return () => {
      // 严格模式的立即重挂载仍使用同一租约，真正卸载或更换实例才释放。
      queueMicrotask(() => {
        if (
          counter.value === current ||
          counter.instanceId !== launch.instanceId
        )
          void transport
            .call("plugin_app_teardown", {
              request: {
                instanceId: launch.instanceId,
                conversationId: launch.conversationId,
              },
            })
            .catch(() => {})
      })
    }
  }, [launch.instanceId, launch.conversationId, transport])
  return transport
}
