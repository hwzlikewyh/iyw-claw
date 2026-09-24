"use client"

import { useTranslations } from "next-intl"
import { useEffect, useState } from "react"
import { Pin, PinOff } from "lucide-react"
import { AgentIcon } from "@/components/agent-icon"
import { Button } from "@/components/ui/button"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { formatConversationTitle } from "@/lib/conversation-title"
import {
  STATUS_ORDER,
  type ConversationStatus,
  type DbConversationSummary,
} from "@/lib/types"
import { formatRelative } from "./sidebar-conversation-grouping"
import { ConversationStatusDot } from "./conversation-status-dot"
import type { ConversationManager } from "./use-conversation-manager"
import { cn } from "@/lib/utils"

const RELATIVE_TIME_REFRESH_MS = 60_000

export function ConversationManagerTable({
  manager,
  onOpen,
}: {
  manager: ConversationManager
  onOpen: (row: DbConversationSummary) => void
}) {
  const t = useTranslations("SidebarDesign")
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const [now, setNow] = useState(Date.now)
  useEffect(() => {
    const timer = window.setInterval(
      () => setNow(Date.now()),
      RELATIVE_TIME_REFRESH_MS
    )
    return () => window.clearInterval(timer)
  }, [])
  return (
    <div className="relative min-h-0 flex-1 overflow-auto rounded-md border">
      <table className="w-full min-w-[42rem] text-left text-xs">
        <thead className="sticky top-0 z-10 bg-muted text-muted-foreground">
          <tr>
            <th className="w-10 px-3 py-2">
              <input
                type="checkbox"
                className="accent-primary"
                aria-label={tm("selectAllVisible")}
                checked={
                  manager.rows.length > 0 &&
                  manager.selectedRows.length === manager.rows.length
                }
                ref={(element) => {
                  if (element)
                    element.indeterminate =
                      manager.selectedRows.length > 0 &&
                      manager.selectedRows.length < manager.rows.length
                }}
                disabled={
                  manager.pending || manager.loading || !manager.rows.length
                }
                onChange={manager.selectAll}
              />
            </th>
            {(["title", "project", "agent", "status", "updated"] as const).map(
              (key) => (
                <th key={key} className="px-2 py-2 font-medium">
                  {t(key)}
                </th>
              )
            )}
            <th>
              <span className="sr-only">{t("pin")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {manager.rows.map((row) => (
            <ManagerRow
              key={row.id}
              row={row}
              manager={manager}
              onOpen={onOpen}
              now={now}
            />
          ))}
        </tbody>
      </table>
      {!manager.rows.length && !manager.error && (
        <div className="p-10 text-center text-sm text-muted-foreground">
          {t(
            manager.loading
              ? "loading"
              : manager.search ||
                  manager.project !== "all" ||
                  manager.agent !== "all" ||
                  manager.status !== "all"
                ? "noMatches"
                : "noSessions"
          )}
        </div>
      )}
    </div>
  )
}

function ManagerRow({
  row,
  manager,
  onOpen,
  now,
}: {
  row: DbConversationSummary
  manager: ConversationManager
  onOpen: (row: DbConversationSummary) => void
  now: number
}) {
  const t = useTranslations("SidebarDesign")
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const ts = useTranslations("Folder.statusLabels")
  const title = formatConversationTitle(row.title) || tm("untitledConversation")
  const pinned = row.pinned_at !== null
  const folder = manager.folders.find((entry) => entry.id === row.folder_id)
  return (
    <tr
      className={cn(
        "border-t hover:bg-muted/40",
        manager.selected.has(row.id) && "bg-primary/5"
      )}
    >
      <td className="px-3 py-3">
        <input
          type="checkbox"
          className="accent-primary"
          aria-label={t("select") + ": " + title}
          checked={manager.selected.has(row.id)}
          disabled={manager.pending || manager.loading}
          onChange={() => manager.select(row.id)}
        />
      </td>
      <td className="max-w-72 px-2 py-3">
        <button
          type="button"
          disabled={manager.pending}
          title={title}
          className="block max-w-full truncate text-left hover:text-primary focus-visible:outline focus-visible:outline-ring"
          onClick={() => onOpen(row)}
        >
          {title}
        </button>
      </td>
      <td
        className="max-w-40 truncate px-2 text-muted-foreground"
        title={folder?.path}
      >
        {row.kind === "chat" ? t("chats") : (folder?.name ?? row.folder_id)}
      </td>
      <td className="px-2">
        <span className="flex items-center gap-1.5 whitespace-nowrap">
          <AgentIcon agentType={row.agent_type} className="size-3.5" />
          {getAgentDisplayName(row.agent_type)}
        </span>
      </td>
      <td className="px-2">
        <span className="flex items-center gap-1.5 whitespace-nowrap">
          <ConversationStatusDot
            status={row.status as ConversationStatus}
            size="sm"
          />
          {STATUS_ORDER.includes(row.status as ConversationStatus)
            ? ts(row.status as ConversationStatus)
            : row.status}
        </span>
      </td>
      <td className="whitespace-nowrap px-2 text-muted-foreground">
        {formatRelative(row.updated_at, now)}
      </td>
      <td className="px-2">
        <Button
          variant="ghost"
          size="icon"
          className={cn("size-7", pinned && "text-primary")}
          title={t(pinned ? "unpin" : "pin")}
          aria-label={t(pinned ? "unpin" : "pin")}
          disabled={manager.pending || manager.loading}
          onClick={() => void manager.run(pinned ? "unpin" : "pin", [row])}
        >
          {pinned ? <PinOff className="size-3" /> : <Pin className="size-3" />}
        </Button>
      </td>
    </tr>
  )
}
