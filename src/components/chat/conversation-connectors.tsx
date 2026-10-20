"use client"

import { useEffect, useRef, useState } from "react"
import { Loader2, Plug } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Switch } from "@/components/ui/switch"
import {
  getConversationConnectors,
  setConversationConnectors,
  prepareConnectorConversation,
  type ConversationConnectors,
} from "@/lib/conversation-connectors"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useConnection } from "@/hooks/use-connection"

export function ConversationConnectorPicker({
  tabId,
}: {
  tabId?: string | null
}) {
  const t = useTranslations("ConversationConnectors")
  const conversationId = useTabStore(
    (state) =>
      state.tabs.find((tab) => tab.id === tabId)?.conversationId ?? null
  )
  const agentType = useTabStore(
    (state) => state.tabs.find((tab) => tab.id === tabId)?.agentType ?? null
  )
  const [open, setOpen] = useState(false)
  const [data, setData] = useState<ConversationConnectors | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const generation = useRef(0)
  const route = useWorkbenchRoute()
  const connection = useConnection(tabId ?? "")
  const reconnect = async () => {
    if (
      busy ||
      connection.status === "prompting" ||
      connection.status === "connecting"
    )
      return
    setBusy(true)
    try {
      await connection.reapplyConfig(true, conversationId ?? undefined)
      setData(await getConversationConnectors(conversationId, agentType))
    } catch (cause) {
      setError(String(cause))
    } finally {
      setBusy(false)
    }
  }
  useEffect(() => {
    const request = ++generation.current
    setData(null)
    setError("")
    setBusy(false)
    if (!open || agentType == null) return
    setBusy(true)
    getConversationConnectors(conversationId, agentType)
      .then((value) => {
        if (generation.current === request) setData(value)
      })
      .catch((cause) => {
        if (generation.current === request) setError(String(cause))
      })
      .finally(() => {
        if (generation.current === request) setBusy(false)
      })
    return () => {
      generation.current = request + 1
    }
  }, [conversationId, agentType, open])
  const toggle = async (id: string, enabled: boolean) => {
    if (!data || !tabId || busy) return
    const request = ++generation.current
    const selected = enabled
      ? [...data.selected, id]
      : data.selected.filter((item) => item !== id)
    setBusy(true)
    setError("")
    try {
      const prepared =
        conversationId == null
          ? await prepareConnectorConversation(tabId, t("newConversation"))
          : null
      const next = await setConversationConnectors({
        conversationId: conversationId ?? prepared!.id,
        revision: data.revision,
        selected,
      })
      if (generation.current === request) setData(next)
      prepared?.bind()
    } catch (cause) {
      if (generation.current === request) setError(String(cause))
    } finally {
      if (generation.current === request) setBusy(false)
    }
  }
  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        if (!busy) setOpen(next)
      }}
    >
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          title={t("title")}
          aria-label={t("title")}
        >
          <Plug className="size-4" />
        </Button>
      </PopoverTrigger>
      <PopoverContent side="top" align="start" className="w-80 space-y-3 p-3">
        <div className="flex items-center justify-between text-xs">
          <strong>{t("title")}</strong>
          <span className="text-muted-foreground">{t("remembered")}</span>
        </div>
        {conversationId == null && (
          <p className="text-xs text-muted-foreground">{t("createFirst")}</p>
        )}
        {busy && !data && (
          <Loader2 className="size-4 animate-spin" aria-label={t("loading")} />
        )}
        {error && (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        )}
        {data?.connectors.map((item) => (
          <div
            key={item.id}
            className="flex items-start justify-between gap-3 py-1"
          >
            <div className="min-w-0">
              <label
                htmlFor={`connector-${conversationId}-${item.id}`}
                className="text-xs font-medium"
              >
                {item.name}
              </label>
              <p className="text-[10px] text-muted-foreground">
                {item.protocol === "sse" ? t("legacySse") : item.protocol}
              </p>
              {item.reason && (
                <p className="text-[10px] text-muted-foreground">
                  {item.reason}
                </p>
              )}
            </div>
            <Switch
              id={`connector-${conversationId}-${item.id}`}
              checked={data.selected.includes(item.id)}
              disabled={
                busy || (!item.compatible && !data.selected.includes(item.id))
              }
              onCheckedChange={(enabled) => void toggle(item.id, enabled)}
            />
          </div>
        ))}
        {data && !data.connectors.length && (
          <p className="text-xs text-muted-foreground">{t("empty")}</p>
        )}
        {data?.reconnect_required && connection.connectionId && (
          <div className="space-y-2 text-xs text-amber-700 dark:text-amber-400">
            <p>{t("reconnect")}</p>
            <Button
              size="sm"
              variant="outline"
              disabled={
                busy ||
                connection.status === "prompting" ||
                connection.status === "connecting" ||
                connection.isViewer ||
                connection.isDelegationChild
              }
              onClick={() => void reconnect()}
            >
              {t("apply")}
            </Button>
          </div>
        )}
        <Button
          size="sm"
          variant="outline"
          className="w-full"
          onClick={() => {
            setOpen(false)
            route.openConnectorMarket()
          }}
        >
          {t("market")}
        </Button>
      </PopoverContent>
    </Popover>
  )
}
