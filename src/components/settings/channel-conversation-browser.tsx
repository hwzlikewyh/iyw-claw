"use client"

import { useDeferredValue, useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { ChatChannelInfo } from "@/lib/types"
import {
  ACTIVE_CHANNEL_PHASES,
  WAITING_CHANNEL_PHASES,
} from "@/lib/channel-runtime"
import type { ChannelConversationInfo } from "@/lib/channel-runtime"
import { useChannelConversationPage } from "@/hooks/use-channel-conversation-page"
import { ChannelConversationDetail } from "./channel-conversation-detail"

export function ChannelConversationBrowser({
  channel,
}: {
  channel: ChatChannelInfo
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const [search, setSearch] = useState("")
  const deferredSearch = useDeferredValue(search)
  return (
    <div className="space-y-3">
      <Input
        value={search}
        onChange={(event) => setSearch(event.target.value)}
        placeholder={t("search")}
        aria-label={t("search")}
      />
      <ConversationBrowser
        key={deferredSearch}
        channelId={channel.id}
        search={deferredSearch}
      />
    </div>
  )
}

function ConversationBrowser({
  channelId,
  search,
}: {
  channelId: number
  search: string
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const page = useChannelConversationPage(channelId, search)
  const [selected, setSelected] = useState<ChannelConversationInfo | null>(null)
  const [filter, setFilter] = useState("all")
  const visible = filterConversations(page.items, filter)
  return (
    <div className="space-y-3">
      <BrowserToolbar
        items={page.items}
        refreshing={page.loading}
        onRefresh={page.refresh}
      />
      <ConversationFilter value={filter} onChange={setFilter} />
      {page.error && (
        <p role="alert" className="text-sm text-destructive">
          {t("refreshFailed")}: {page.error}
        </p>
      )}
      <div className="grid min-h-[32rem] overflow-hidden rounded-lg border lg:grid-cols-[18rem_minmax(0,1fr)]">
        <ConversationList
          items={visible}
          selected={selected}
          onSelect={setSelected}
          page={page}
        />
        {selected ? (
          <ChannelConversationDetail
            key={selected.conversationId}
            channelId={channelId}
            item={selected}
            onBack={() => setSelected(null)}
          />
        ) : (
          <p className="hidden p-8 text-sm text-muted-foreground lg:block">
            {t("selectConversation")}
          </p>
        )}
      </div>
    </div>
  )
}

function filterConversations(items: ChannelConversationInfo[], filter: string) {
  return items.filter((item) => {
    if (filter === "all") return true
    if (filter === "active") return ACTIVE_CHANNEL_PHASES.has(item.phase)
    if (filter === "waiting") return WAITING_CHANNEL_PHASES.has(item.phase)
    return ["failed", "stopped", "disconnected", "unknown"].includes(item.phase)
  })
}

function ConversationFilter({
  value,
  onChange,
}: {
  value: string
  onChange: (value: string) => void
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const filters = ["all", "active", "waiting", "attention"] as const
  return (
    <label className="flex items-center gap-2 text-xs text-muted-foreground">
      {t("filter")}
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-8 rounded-md border bg-background px-2 text-foreground"
      >
        {filters.map((filter) => (
          <option key={filter} value={filter}>
            {t(`filters.${filter}`)}
          </option>
        ))}
      </select>
    </label>
  )
}

function ConversationList({
  items,
  selected,
  onSelect,
  page,
}: {
  items: ChannelConversationInfo[]
  selected: ChannelConversationInfo | null
  onSelect: (item: ChannelConversationInfo) => void
  page: ReturnType<typeof useChannelConversationPage>
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <div
      className={`max-h-[70vh] overflow-auto border-r ${selected ? "hidden lg:block" : ""}`}
    >
      <ConversationRows
        items={items}
        selected={selected?.conversationId}
        onSelect={onSelect}
        loading={page.loading}
      />
      {page.before && (
        <Button
          variant="ghost"
          className="w-full"
          disabled={page.moreLoading}
          onClick={() => void page.loadMore()}
        >
          {page.moreLoading && <Loader2 className="size-4 animate-spin" />}
          {t("loadMore")}
        </Button>
      )}
    </div>
  )
}
function BrowserToolbar({
  items,
  refreshing,
  onRefresh,
}: {
  items: ChannelConversationInfo[]
  refreshing: boolean
  onRefresh: () => void
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <div className="flex items-center justify-between gap-3">
      <p className="text-xs text-muted-foreground">
        {t("counts", {
          count: items.length,
          running: items.filter((item) => ACTIVE_CHANNEL_PHASES.has(item.phase))
            .length,
          waiting: items.filter((item) =>
            WAITING_CHANNEL_PHASES.has(item.phase)
          ).length,
        })}
      </p>
      <Button
        variant="outline"
        size="sm"
        onClick={onRefresh}
        disabled={refreshing}
      >
        <RefreshCw className="size-3.5" />
        {t("refresh")}
      </Button>
    </div>
  )
}

function ConversationRows({
  items,
  selected,
  onSelect,
  loading,
}: {
  items: ChannelConversationInfo[]
  selected?: number
  onSelect: (item: ChannelConversationInfo) => void
  loading: boolean
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  if (!items.length)
    return (
      <p role="status" className="p-6 text-sm text-muted-foreground">
        {t(loading ? "loading" : "noConversations")}
      </p>
    )
  return (
    <ul className="divide-y">
      {items.map((item) => (
        <li key={item.conversationId}>
          <button
            type="button"
            aria-pressed={item.conversationId === selected}
            onClick={() => onSelect(item)}
            className={`w-full space-y-1.5 px-4 py-3 text-left hover:bg-muted/50 ${item.conversationId === selected ? "bg-muted" : ""}`}
          >
            <p className="truncate text-sm font-medium">
              {item.title || t("untitled")}
            </p>
            <p className="text-xs text-muted-foreground">
              {t(`phases.${item.phase}`)} ·{" "}
              {t("messageCount", { count: item.messageCount })}
            </p>
            <p className="text-[11px] text-muted-foreground">
              {new Date(item.updatedAt).toLocaleString()}
            </p>
          </button>
        </li>
      ))}
    </ul>
  )
}
