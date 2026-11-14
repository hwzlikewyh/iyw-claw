import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { getTransport } from "@/lib/transport"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  launchPluginPanel,
  type PluginPanelChoice,
  type PluginPanelSyncState,
  type PluginPanelUpdate,
} from "@/lib/plugin-panel"
import { requestPluginAppTeardown } from "@/lib/plugin-app-teardown"
import { reportPluginWindowReady } from "@/lib/plugin-panel-window"
import type { PluginPanelTab } from "./use-plugin-panel-tabs"
import { connectPluginWindowRelay } from "@/lib/plugin-window-relay"

export function usePluginWindowRelay(relayId: string, conversationId: number) {
  const [ready, setReady] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => {
      void reportPluginWindowReady("原对话窗口未响应，请重试").catch(
        console.error
      )
    }, 30_000)
    const stop = connectPluginWindowRelay(relayId, conversationId, () => {
      clearTimeout(timer)
      setReady(true)
    })
    return () => {
      clearTimeout(timer)
      stop()
    }
  }, [relayId, conversationId])
  return ready
}

const SYNC_INTERVAL_MS = 5_000

export function useDetachedPluginLoad(
  input: PluginPanelChoice & { conversationId: number },
  enabled = true
) {
  const [tab, setTab] = useState<PluginPanelTab | null>(null)
  const [error, setError] = useState("")
  useEffect(() => {
    if (!enabled) return
    let alive = true
    const transport = getTransport()
    const timer = setTimeout(() => {
      void launchPluginPanel(input.conversationId, input, transport)
        .then(async (response) => {
          if (!alive) {
            await transport.call("plugin_app_teardown", {
              request: {
                instanceId: response.launch.instanceId,
                conversationId: input.conversationId,
              },
            })
            return
          }
          setTab({
            id: response.launch.instanceId,
            label: input.label,
            conversationId: input.conversationId,
            pluginSlug: input.pluginSlug,
            appKey: input.appKey,
            response,
          })
        })
        .catch((cause) => {
          const message = toErrorMessage(cause)
          if (alive) setError(message)
          void reportPluginWindowReady(message).catch(console.error)
        })
    }, 0)
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [input, enabled])
  return { tab, error }
}

export function usePluginWindowClose(tab: PluginPanelTab) {
  const [error, setError] = useState("")
  const t = useTranslations("PluginPanels")
  useEffect(() => {
    let disposed = false,
      closing = false,
      allowed = false
    let unlisten: (() => void) | undefined
    const transport = getTransport()
    void import("@tauri-apps/api/window")
      .then(async ({ getCurrentWindow }) => {
        const win = getCurrentWindow()
        const stop = await win.onCloseRequested(async (event) => {
          if (allowed) return
          event.preventDefault()
          if (closing) return
          closing = true
          try {
            if (!(await requestPluginAppTeardown(tab.id)))
              throw Error(t("closeFailed"))
            await transport.call("plugin_app_teardown", {
              request: {
                instanceId: tab.id,
                conversationId: tab.conversationId,
              },
            })
            allowed = true
            await win.close()
          } catch (cause) {
            if (!disposed) setError(toErrorMessage(cause))
          } finally {
            closing = false
          }
        })
        if (disposed) stop()
        else unlisten = stop
      })
      .catch((cause) => {
        if (!disposed) setError(toErrorMessage(cause))
      })
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [tab, t])
  return error
}

export function useDetachedPluginSync(tab: PluginPanelTab) {
  const [state, setState] = useState<PluginPanelSyncState>({})
  useEffect(() => {
    let alive = true
    let timer: ReturnType<typeof setTimeout>
    const transport = getTransport()
    const sync = async () => {
      let forbidden = false
      try {
        const update = await transport.call<PluginPanelUpdate | null>(
          "plugin_app_sync",
          {
            request: {
              instanceId: tab.id,
              conversationId: tab.conversationId,
              leaseToken: tab.response.launch.leaseToken,
              nonce: tab.response.launch.nonce,
            },
          }
        )
        if (alive)
          setState((value) => ({
            update:
              update && (!value.update || update.cursor > value.update.cursor)
                ? update
                : value.update,
          }))
      } catch (cause) {
        forbidden = ["permission_denied", "configuration_missing"].includes(
          extractAppCommandError(cause)?.code ?? ""
        )
        if (alive) setState((value) => ({ ...value, error: true, forbidden }))
      }
      if (alive && !forbidden) timer = setTimeout(sync, SYNC_INTERVAL_MS)
    }
    void sync()
    return () => {
      alive = false
      clearTimeout(timer)
    }
  }, [tab])
  return state
}
