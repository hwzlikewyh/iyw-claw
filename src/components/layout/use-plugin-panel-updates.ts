import { useEffect, useRef, useState } from "react"
import { useAcpEvent } from "@/contexts/acp-connections-context"
import { extractAppCommandError } from "@/lib/app-error"
import type { getTransport } from "@/lib/transport"
import type {
  PluginPanelSyncState,
  PluginPanelUpdate,
} from "@/lib/plugin-panel"
import type { PluginPanelTab } from "./use-plugin-panels"

const SYNC_DELAY_MS = 40
type States = Record<string, PluginPanelSyncState>

export function usePluginPanelUpdates(
  tabs: PluginPanelTab[],
  transport: ReturnType<typeof getTransport>
) {
  const [states, setStates] = useState<States>({})
  const schedule = useRef(() => {})
  const blocked = useRef(new Set<string>())
  useEffect(() => {
    let alive = true,
      timer: ReturnType<typeof setTimeout>
    const requests = new Map<string, number>()
    schedule.current = () => {
      clearTimeout(timer)
      timer = setTimeout(() => {
        const ids = new Set(tabs.map((tab) => tab.id))
        for (const id of blocked.current)
          if (!ids.has(id)) blocked.current.delete(id)
        setStates((current) =>
          Object.fromEntries(
            Object.entries(current).filter(([id]) => ids.has(id))
          )
        )
        for (const tab of tabs.filter((tab) => !blocked.current.has(tab.id)))
          void loadUpdate(tab, transport, {
            requests,
            alive: () => alive,
            commit: (value) => {
              if (value.forbidden) blocked.current.add(tab.id)
              setStates((current) => merge(current, tab.id, value))
            },
          })
      }, SYNC_DELAY_MS)
    }
    schedule.current()
    return () => {
      alive = false
      clearTimeout(timer)
      schedule.current = () => {}
    }
  }, [tabs, transport])
  useAcpEvent((event) => {
    if (event.type !== "tool_call" && event.type !== "tool_call_update") return
    const meta = event.meta as Record<string, unknown> | undefined
    const app = meta?.["iyw-claw.plugin-app"] as
      | Record<string, unknown>
      | undefined
    if (typeof app?.instanceId === "string") schedule.current()
  })
  return { states, refresh: () => schedule.current() }
}

async function loadUpdate(
  tab: PluginPanelTab,
  transport: ReturnType<typeof getTransport>,
  lifecycle: {
    requests: Map<string, number>
    alive: () => boolean
    commit: (value: PluginPanelSyncState) => void
  }
) {
  const generation = (lifecycle.requests.get(tab.id) ?? 0) + 1
  lifecycle.requests.set(tab.id, generation)
  const current = () =>
    lifecycle.alive() && lifecycle.requests.get(tab.id) === generation
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
    if (current()) lifecycle.commit(update ? { update } : {})
  } catch (cause) {
    const code = extractAppCommandError(cause)?.code
    if (current())
      lifecycle.commit({
        error: true,
        forbidden:
          code === "permission_denied" || code === "configuration_missing",
      })
  }
}

function merge(
  states: States,
  id: string,
  value: PluginPanelSyncState
): States {
  const previous = states[id]
  if (previous?.forbidden) return states
  if (
    value.update &&
    previous?.update &&
    value.update.cursor <= previous.update.cursor &&
    !previous.error
  )
    return states
  const update =
    value.update &&
    (!previous?.update || value.update.cursor > previous.update.cursor)
      ? value.update
      : previous?.update
  return { ...states, [id]: { ...value, update } }
}
