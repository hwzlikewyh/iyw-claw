"use client"

import { formatDistanceToNow } from "date-fns"
import { enUS, zhCN } from "date-fns/locale"
import { Loader2, RefreshCw } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { formatConversationTitle } from "@/lib/conversation-title"
import type {
  AgentType,
  ConversationStatus,
  DbConversationSummary,
} from "@/lib/types"
import { ConversationStatusDot } from "./conversation-status-dot"
import { Button } from "@/components/ui/button"
import {
  CommandEmpty,
  CommandGroup,
  CommandItem,
} from "@/components/ui/command"
import { useConversationSearch } from "./use-command-search"

export function ConversationSearchResults({
  query,
  folderId,
  agent,
  onClose,
}: {
  query: string
  folderId: number | null
  agent: AgentType | null
  onClose: () => void
}) {
  const t = useTranslations("Folder.search")
  const search = useConversationSearch({ query, folderId, agent })
  const { openTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const select = (item: DbConversationSummary) => {
    openConversations()
    openTab(item.folder_id, item.id, item.agent_type, true)
    onClose()
  }
  if (search.loading || search.error) return <SearchRequestStatus {...search} />
  return (
    <>
      <CommandEmpty>
        {!query.trim() && !agent ? t("typeToSearch") : t("noResults")}
      </CommandEmpty>
      {!!search.data?.length && (
        <CommandGroup>
          {search.data.map((item) => (
            <ConversationSearchRow
              key={item.id}
              item={item}
              onSelect={() => select(item)}
            />
          ))}
        </CommandGroup>
      )}
    </>
  )
}

function ConversationSearchRow({
  item,
  onSelect,
}: {
  item: DbConversationSummary
  onSelect: () => void
}) {
  const t = useTranslations("Folder.search")
  const locale = useLocale()
  return (
    <CommandItem value={`conversation:${item.id}`} onSelect={onSelect}>
      <ConversationStatusDot status={item.status as ConversationStatus} />
      <span className="min-w-0 flex-1 truncate">
        {formatConversationTitle(item.title) || t("untitledConversation")}
      </span>
      <span className="max-w-24 truncate text-xs text-muted-foreground">
        {getAgentDisplayName(item.agent_type)}
      </span>
      <span className="hidden shrink-0 text-xs text-muted-foreground sm:inline">
        {formatDistanceToNow(new Date(item.created_at), {
          addSuffix: true,
          locale: locale === "zh-CN" ? zhCN : enUS,
        })}
      </span>
    </CommandItem>
  )
}

export function SearchRequestStatus({
  loading,
  error,
  retry,
}: {
  loading: boolean
  error: boolean
  retry: () => void
}) {
  const t = useTranslations("Folder.search")
  const common = useTranslations("Folder.taskArtifacts")
  if (!loading && !error) return null
  return (
    <div
      role={error ? "alert" : "status"}
      className="flex items-center justify-center gap-2 px-4 py-6 text-sm text-muted-foreground"
    >
      {loading ? (
        <>
          <Loader2 className="size-4 animate-spin" />
          {t("searching")}
        </>
      ) : (
        <>
          <span>{t("searchFailed")}</span>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={retry}
            title={common("retry")}
            aria-label={common("retry")}
          >
            <RefreshCw className="size-4" />
          </Button>
        </>
      )}
    </div>
  )
}
