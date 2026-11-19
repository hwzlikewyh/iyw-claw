"use client"

import { useEffect, useState } from "react"
import { useTabStore } from "@/contexts/tab-context"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  getConversationConnectors,
  onConversationConnectorsChanged,
  type ConversationConnectors,
} from "@/lib/conversation-connectors"
import { getTransport, onTransportChange } from "@/lib/transport"
import { onPluginPreparation } from "@/lib/plugin-preparation-events"
import { ComposerSourceChip } from "./composer-source-chip"
import { useConnectorIcons } from "./use-connector-icons"

const VISIBLE_CONNECTORS = 3
export function ConversationConnectorComposerChips({
  tabId,
}: {
  tabId?: string | null
}) {
  const id = useTabStore(
    (state) =>
      state.tabs.find((tab) => tab.id === tabId)?.conversationId ?? null
  )
  const connectors = useSelectedConnectors(id)
  const icons = useConnectorIcons(connectors.length > 0)
  const hidden = connectors.slice(VISIBLE_CONNECTORS)
  const names = hidden.map((item) => item.name).join(", ")
  return (
    <>
      {connectors.slice(0, VISIBLE_CONNECTORS).map((item) => (
        <ComposerSourceChip
          key={item.id}
          kind="connector"
          label={item.name}
          src={icons.get(item.id)}
        />
      ))}
      {hidden.length > 0 && (
        <Tooltip>
          <TooltipTrigger asChild>
            <span
              tabIndex={0}
              aria-label={names}
              className="inline-flex h-7 items-center rounded-md border border-border/70 bg-muted/45 px-2 text-xs text-muted-foreground"
            >
              +{hidden.length}
            </span>
          </TooltipTrigger>
          <TooltipContent
            side="top"
            sideOffset={4}
            className="max-h-48 max-w-[min(18rem,90vw)] overflow-y-auto break-all rounded-md"
          >
            {names}
          </TooltipContent>
        </Tooltip>
      )}
    </>
  )
}

function useSelectedConnectors(id: number | null) {
  const [transport, setTransport] = useState(getTransport)
  useEffect(() => onTransportChange(() => setTransport(getTransport())), [])
  const [snapshot, setSnapshot] = useState<{
    id: number
    transport: typeof transport
    data: ConversationConnectors
  } | null>(null)
  useEffect(() => {
    if (id == null) return
    let serial = 0
    const apply = (data: ConversationConnectors) => {
      if (getTransport() === transport) setSnapshot({ id, transport, data })
    }
    const refresh = () => {
      const request = ++serial
      void getConversationConnectors(id, null)
        .then((data) => {
          if (request === serial) apply(data)
        })
        .catch(() => {
          if (request === serial) setSnapshot(null)
        })
    }
    refresh()
    const offChange = onConversationConnectorsChanged(id, (data) => {
      serial++
      apply(data)
    })
    const offPlugin = onPluginPreparation((event) => {
      if (event.conversationId === id) refresh()
    })
    window.addEventListener("focus", refresh)
    return () => {
      serial++
      offChange()
      offPlugin()
      window.removeEventListener("focus", refresh)
    }
  }, [id, transport])
  if (!snapshot || snapshot.id !== id || snapshot.transport !== transport)
    return []
  return snapshot.data.connectors.filter((item) =>
    snapshot.data.selected.includes(item.id)
  )
}
