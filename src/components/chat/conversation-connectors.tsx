"use client"

import { useEffect, useRef, useState } from "react"
import { Link2, Loader2, Search, Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/contexts/tab-context"
import {
  DropdownMenuCheckboxItem,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import {
  getConversationConnectors,
  setConversationConnectors,
  prepareConnectorConversation,
  type ConversationConnectors,
} from "@/lib/conversation-connectors"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useConnection } from "@/hooks/use-connection"

export function ConversationConnectorMenu({
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
  const [query, setQuery] = useState("")
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
  const normalizedQuery = query.trim().toLocaleLowerCase()
  const visibleConnectors =
    data?.connectors.filter((item) =>
      item.name.toLocaleLowerCase().includes(normalizedQuery)
    ) ?? []
  return (
    <DropdownMenuSub
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (!next) setQuery("")
      }}
    >
      <DropdownMenuSubTrigger disabled={!agentType}>
        <Link2 className="size-4" />
        {t("menuLabel")}
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent
        className="flex w-72 max-w-[calc(100vw-1rem)] flex-col p-1"
        style={{
          maxHeight:
            "min(32rem, var(--radix-dropdown-menu-content-available-height))",
        }}
        aria-label={t("title")}
      >
        <div className="relative m-1 shrink-0">
          <Search className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("search")}
            aria-label={t("search")}
            className="h-9 pl-9"
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault()
                event.currentTarget
                  .closest('[role="menu"]')
                  ?.querySelector<HTMLElement>(
                    '[role="menuitemcheckbox"]:not([data-disabled])'
                  )
                  ?.focus()
              }
              if (event.key !== "Escape" && event.key !== "Tab")
                event.stopPropagation()
            }}
          />
        </div>
        <p className="px-3 py-1 text-[10px] text-muted-foreground">
          {t("remembered")}
        </p>
        {conversationId == null && (
          <p className="px-3 py-1 text-xs text-muted-foreground">
            {t("createFirst")}
          </p>
        )}
        {busy && !data && (
          <Loader2
            className="m-3 size-4 animate-spin"
            aria-label={t("loading")}
          />
        )}
        {error && (
          <p role="alert" className="px-3 py-1 text-xs text-destructive">
            {error}
          </p>
        )}
        <div className="min-h-0 overflow-y-auto">
          {visibleConnectors.map((item) => (
            <DropdownMenuCheckboxItem
              key={item.id}
              className="gap-3 pr-3 [&_[data-slot=dropdown-menu-checkbox-item-indicator]]:hidden"
              checked={data!.selected.includes(item.id)}
              disabled={
                busy || (!item.compatible && !data!.selected.includes(item.id))
              }
              onSelect={(event) => event.preventDefault()}
              onCheckedChange={(enabled) => void toggle(item.id, enabled)}
            >
              <Link2 className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <span className="block truncate text-sm font-medium">
                  {item.name}
                </span>
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
                aria-hidden="true"
                tabIndex={-1}
                className="pointer-events-none"
                checked={data!.selected.includes(item.id)}
              />
            </DropdownMenuCheckboxItem>
          ))}
          {data && !data.connectors.length && (
            <p className="px-3 py-3 text-xs text-muted-foreground">
              {t("empty")}
            </p>
          )}
          {data &&
            data.connectors.length > 0 &&
            visibleConnectors.length === 0 && (
              <p className="px-3 py-3 text-xs text-muted-foreground">
                {t("noResults")}
              </p>
            )}
        </div>
        {data?.reconnect_required && connection.connectionId && (
          <div className="shrink-0 text-xs text-amber-700 dark:text-amber-400">
            <p className="px-3 py-2">{t("reconnect")}</p>
            <DropdownMenuItem
              disabled={
                busy ||
                connection.status === "prompting" ||
                connection.status === "connecting" ||
                connection.isViewer ||
                connection.isDelegationChild
              }
              onSelect={(event) => {
                event.preventDefault()
                void reconnect()
              }}
            >
              {t("apply")}
            </DropdownMenuItem>
          </div>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          className="shrink-0"
          onSelect={() => {
            setOpen(false)
            route.openConnectorMarket()
          }}
        >
          <Settings2 className="size-4" />
          {t("market")}
        </DropdownMenuItem>
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  )
}
