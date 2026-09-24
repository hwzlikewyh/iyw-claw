"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import {
  ArrowDownToLine,
  ArrowUpToLine,
  Check,
  Copy,
  Download,
  FileCode2,
  FileText,
  ListChecks,
  MessageSquareText,
  PencilLine,
  Search,
  Star,
  Trash,
  Workflow,
} from "lucide-react"
import { ContextMenuContent } from "@/components/ui/context-menu"
import {
  STATUS_COLORS,
  STATUS_ORDER,
  type ConversationStatus,
  type DbConversationSummary,
} from "@/lib/types"
import { formatConversationTitle } from "@/lib/conversation-title"
import { cn } from "@/lib/utils"
import {
  ConversationMenuItem as Item,
  ConversationMenuSeparator as Separator,
  ConversationMenuSub as Submenu,
  MENU_CONTENT_CLASS,
} from "./conversation-menu-parts"
import { useConversationMenuActions } from "./use-conversation-menu-actions"
import { ConversationMessageSearchDialog } from "./conversation-message-search-dialog"

interface ConversationMenuProps {
  conversation: DbConversationSummary
  favorite: boolean
  setFavorite: (favorite: boolean) => void
  onRename: () => void
  onDelete: () => void
  onDetails: () => void
  onTogglePin?: (id: number, pinned: boolean) => void
  onStatusChange: (id: number, status: ConversationStatus) => Promise<void>
  onAddToAutomation?: (id: number) => void
}

export function ConversationMenu(props: ConversationMenuProps) {
  const t = useTranslations("Folder.conversationMenu")
  const [searchOpen, setSearchOpen] = useState(false)
  const actions = useConversationMenuActions(props.conversation)
  return (
    <>
      <ContextMenuContent className={MENU_CONTENT_CLASS}>
        <div
          className="px-2.5 py-2 text-[0.8125rem] font-medium truncate"
          title={props.conversation.title ?? undefined}
        >
          {formatConversationTitle(props.conversation.title) || t("untitled")}
        </div>
        <OrganizeItems {...props} />
        <Separator />
        <Item icon={Search} onSelect={() => setSearchOpen(true)}>
          {t("searchMessages")}
        </Item>
        <ContentItems actions={actions} />
        <Separator />
        <StatusItems {...props} />
        {props.onAddToAutomation && props.conversation.parent_id == null && (
          <Item
            icon={Workflow}
            onSelect={() => props.onAddToAutomation?.(props.conversation.id)}
          >
            {t("automation")}
          </Item>
        )}
        <Item icon={FileText} onSelect={props.onDetails}>
          {t("details")}
        </Item>
        <Separator />
        <Item icon={Trash} variant="destructive" onSelect={props.onDelete}>
          {t("delete")}
        </Item>
      </ContextMenuContent>
      {searchOpen && (
        <ConversationMessageSearchDialog
          key={props.conversation.id}
          summary={props.conversation}
          onOpenChange={setSearchOpen}
        />
      )}
    </>
  )
}

function OrganizeItems(props: ConversationMenuProps) {
  const t = useTranslations("Folder.conversationMenu")
  const pinned = props.conversation.pinned_at != null
  const toggleFavorite = () => {
    try {
      props.setFavorite(!props.favorite)
    } catch (error) {
      console.error("[conversation-menu] favorite save failed", {
        conversationId: props.conversation.id,
        error,
      })
      toast.error(t("favoriteFailed"))
    }
  }
  return (
    <>
      {props.onTogglePin && (
        <Item
          icon={pinned ? ArrowDownToLine : ArrowUpToLine}
          onSelect={() => props.onTogglePin?.(props.conversation.id, !pinned)}
        >
          {t(pinned ? "unpin" : "pin")}
        </Item>
      )}
      <Item
        icon={Star}
        onSelect={toggleFavorite}
        iconClassName={
          props.favorite
            ? "fill-amber-400/20 text-amber-600 dark:text-amber-400"
            : undefined
        }
      >
        {t(props.favorite ? "unfavorite" : "favorite")}
      </Item>
      <Item icon={PencilLine} onSelect={props.onRename}>
        {t("rename")}
      </Item>
    </>
  )
}

function ContentItems({
  actions,
}: {
  actions: ReturnType<typeof useConversationMenuActions>
}) {
  const t = useTranslations("Folder.conversationMenu")
  return (
    <>
      <Submenu icon={Copy} label={t("copy")} disabled={actions.busy}>
        <Item icon={PencilLine} onSelect={() => void actions.run("copyTitle")}>
          {t("copyTitle")}
        </Item>
        <Item
          icon={MessageSquareText}
          onSelect={() => void actions.run("copyContent")}
        >
          {t("copyContent")}
        </Item>
      </Submenu>
      <Submenu icon={Download} label={t("export")} disabled={actions.busy}>
        <Item icon={FileText} onSelect={() => void actions.run("markdown")}>
          Markdown
        </Item>
        <Item icon={FileCode2} onSelect={() => void actions.run("html")}>
          HTML
        </Item>
      </Submenu>
    </>
  )
}

function StatusItems(props: ConversationMenuProps) {
  const t = useTranslations("Folder.conversationMenu")
  const statuses = useTranslations("Folder.statusLabels")
  const current = props.conversation.status as ConversationStatus
  const changeStatus = async (status: ConversationStatus) => {
    try {
      await props.onStatusChange(props.conversation.id, status)
    } catch (error) {
      console.error("[conversation-menu] status change failed", {
        conversationId: props.conversation.id,
        status,
        error,
      })
      toast.error(t("actionFailed"))
    }
  }
  return (
    <Submenu
      icon={ListChecks}
      label={t("changeStatus")}
      value={STATUS_ORDER.includes(current) ? statuses(current) : undefined}
    >
      {STATUS_ORDER.map((status) => (
        <Item
          key={status}
          role="menuitemradio"
          aria-checked={status === current}
          onSelect={() => {
            if (status !== current) void changeStatus(status)
          }}
        >
          <span className="flex size-4 shrink-0 items-center justify-center">
            <span
              className={cn("size-2 rounded-full", STATUS_COLORS[status])}
            />
          </span>
          {statuses(status)}
          <Check
            aria-hidden
            className={cn(
              "ml-auto size-3.5",
              status !== current && "invisible"
            )}
          />
        </Item>
      ))}
    </Submenu>
  )
}
