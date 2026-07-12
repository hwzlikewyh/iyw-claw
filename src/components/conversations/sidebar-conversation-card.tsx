"use client"

import { memo, useState, useCallback, type CSSProperties } from "react"
import {
  Pencil,
  Trash2,
  Circle,
  SquarePen,
  Loader2,
  XCircle,
  Pin,
  PinOff,
  CheckCircle2,
  Info,
  ChevronRight,
} from "lucide-react"
import { useTranslations } from "next-intl"
import type { DbConversationSummary, ConversationStatus } from "@/lib/types"
import { STATUS_ORDER } from "@/lib/types"
import { cn } from "@/lib/utils"
import { formatConversationTitle } from "@/lib/conversation-title"
import {
  ContextMenu,
  ContextMenuTrigger,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSub,
  ContextMenuSubTrigger,
  ContextMenuSubContent,
  ContextMenuSeparator,
} from "@/components/ui/context-menu"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { ConversationStatusDot } from "./conversation-status-dot"
import { SessionDetailsDialog } from "./session-details-dialog"
import { SessionUsageChip } from "@/components/layout/status-bar-tokens"
import { useConversationRuntimeStore } from "@/stores/conversation-runtime-store"

/**
 * Horizontal indent added per delegation-nesting level. The root axis
 * (`0.875rem`) and the axis→title gap (`0.875rem`) are separate constants —
 * don't fold them into this step.
 */
export const CONV_RAIL_DEPTH_STEP = "1.25rem"
const SHOW_SIDEBAR_SESSION_USAGE = false

/**
 * Vertical guide rails for a delegation sub-session's ANCESTORS. A row at `depth`
 * draws one rail per ancestor level (axis 0 … depth−1), each a 2px line at
 * `axis(level) = 0.875rem + level·CONV_RAIL_DEPTH_STEP` from the row's left edge
 * — the same x as that ancestor row's own rail. Stacked across a contiguous
 * subtree they render each parent's rail as a single continuous vertical line
 * running down through all of its descendants, so a child's left rail lines up
 * exactly under its parent's instead of floating one indent step to the right.
 * The row's OWN rail is drawn separately at `--conv-rail-axis` by the caller.
 *
 * Renders nothing for a root (depth 0). Shared with the list's sub-session
 * loading placeholder so the spine stays continuous while children are fetched.
 */
export function SubsessionAncestorRails({ depth }: { depth: number }) {
  if (depth <= 0) return null
  return (
    <>
      {Array.from({ length: depth }, (_, level) => (
        <span
          key={level}
          aria-hidden
          data-subsession-rail
          className="pointer-events-none absolute z-0 bg-sidebar-border"
          style={{
            top: "-0.0625rem",
            bottom: "-0.0625rem",
            left: `calc(0.875rem + ${level} * ${CONV_RAIL_DEPTH_STEP})`,
            width: "0.125rem",
            transform: "translateX(-50%)",
          }}
        />
      ))}
    </>
  )
}

interface SidebarConversationCardProps {
  conversation: DbConversationSummary
  isSelected: boolean
  isOpenInTab?: boolean
  timeLabel?: string
  onSelect: (id: number, agentType: string, folderId: number) => void
  onDoubleClick?: (id: number, agentType: string, folderId: number) => void
  onRename: (id: number, newTitle: string) => Promise<void>
  onDelete: (id: number, agentType: string, folderId: number) => Promise<void>
  onStatusChange: (id: number, status: ConversationStatus) => Promise<void>
  onNewConversation?: (folderId: number) => void
  onTogglePin?: (id: number, nextPinned: boolean) => void
  openContextKey?: string | null
  runtimeConversationId?: number | null
  /** Delegation-tree nesting depth (0 = root). Drives the per-level indent. */
  depth?: number
  /** True when `child_count > 0`: the conversation has delegation children, so
   *  the expand chevron is shown. */
  hasChildren?: boolean
  /** Whether this conversation's sub-session subtree is currently expanded. */
  expanded?: boolean
  /** Toggle this conversation's sub-session subtree (lazily loads on expand). */
  onToggleExpand?: (id: number) => void
}

export const SidebarConversationCard = memo(function SidebarConversationCard({
  conversation,
  isSelected,
  isOpenInTab = false,
  timeLabel,
  onSelect,
  onDoubleClick,
  onRename,
  onDelete,
  onStatusChange,
  onNewConversation,
  onTogglePin,
  openContextKey,
  runtimeConversationId,
  depth = 0,
  hasChildren = false,
  expanded = false,
  onToggleExpand,
}: SidebarConversationCardProps) {
  const t = useTranslations("Folder.conversationCard")
  const tSidebar = useTranslations("Folder.sidebar")
  const tStatus = useTranslations("Folder.statusLabels")
  const tDetails = useTranslations("Folder.sessionDetails")
  const [renameOpen, setRenameOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [detailsOpen, setDetailsOpen] = useState(false)
  const [renameValue, setRenameValue] = useState("")

  const handleClick = useCallback(() => {
    onSelect(conversation.id, conversation.agent_type, conversation.folder_id)
  }, [
    onSelect,
    conversation.id,
    conversation.agent_type,
    conversation.folder_id,
  ])

  const handleDblClick = useCallback(() => {
    onDoubleClick?.(
      conversation.id,
      conversation.agent_type,
      conversation.folder_id
    )
  }, [
    onDoubleClick,
    conversation.id,
    conversation.agent_type,
    conversation.folder_id,
  ])

  const handleRenameOpen = useCallback(() => {
    setRenameValue(conversation.title || "")
    setRenameOpen(true)
  }, [conversation.title])

  const handleRenameConfirm = useCallback(async () => {
    const trimmed = renameValue.trim()
    if (trimmed && trimmed !== conversation.title) {
      await onRename(conversation.id, trimmed)
    }
    setRenameOpen(false)
  }, [renameValue, conversation.id, conversation.title, onRename])

  const handleDeleteConfirm = useCallback(async () => {
    await onDelete(
      conversation.id,
      conversation.agent_type,
      conversation.folder_id
    )
    setDeleteOpen(false)
  }, [
    conversation.id,
    conversation.agent_type,
    conversation.folder_id,
    onDelete,
  ])

  const status = conversation.status as ConversationStatus
  const isRunning = status === "in_progress"
  const isCancelled = status === "cancelled"
  const isPinned = conversation.pinned_at != null
  const isCompleted = status === "completed"
  // Delegation sub-sessions (a child of another conversation) don't get the
  // hover quick actions: pinning a sub-agent run to the root Pinned section or
  // hand-toggling its status doesn't fit — its lifecycle is the sub-agent's. The
  // time / running badge then stays visible on hover (nothing swaps in for it).
  const isSubsession = conversation.parent_id != null

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          <div
            className="relative h-[2rem] bg-sidebar"
            data-conv-key={`${conversation.agent_type}:${conversation.id}`}
            // Per-level indent: shift the shared rail axis right by one step per
            // depth. Root rows (depth 0) leave the var untouched so they inherit
            // the list's `--conv-rail-axis: 0.875rem` and render exactly as
            // before; the rail and button padding both key off this var so the
            // whole row indents cohesively.
            style={
              depth > 0
                ? ({
                    "--conv-rail-axis": `calc(0.875rem + ${depth} * ${CONV_RAIL_DEPTH_STEP})`,
                  } as CSSProperties)
                : undefined
            }
          >
            <div
              className={cn(
                "group relative flex h-[1.9375rem] w-full items-center",
                "rounded-lg text-sidebar-foreground",
                "transition-colors duration-[120ms]",
                isSelected
                  ? "bg-sidebar-accent/80 before:absolute before:top-1.5 before:bottom-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-primary"
                  : "hover:bg-sidebar-accent/55"
              )}
            >
              <button
                data-conversation-id={conversation.id}
                onClick={handleClick}
                onDoubleClick={handleDblClick}
                className={cn(
                  "relative flex h-full min-w-0 flex-1 items-center gap-[0.625rem] text-left outline-none",
                  "rounded-lg",
                  "pr-[0.25rem]"
                )}
                // Rail-axis-relative left padding (was a fixed `pl-7`): at depth 0
                // this resolves to 0.875rem + 0.875rem = 1.75rem (= pl-7), so root
                // rows are pixel-identical; deeper rows inherit the shifted var.
                style={{
                  paddingLeft:
                    "calc(var(--conv-rail-axis, 0.875rem) + 0.875rem)",
                }}
              >
                {/* Ancestor guide rails (depth ≥ 1): keep each parent's vertical
                    line continuous down through this nested row, so the child's
                    left rail aligns under the parent's. */}
                <SubsessionAncestorRails depth={depth} />
                {/* This row's OWN rail at the depth-shifted rail axis. */}
                <span
                  aria-hidden
                  className={cn(
                    "pointer-events-none absolute z-0 bg-sidebar-border"
                  )}
                  style={{
                    top: "-0.0625rem",
                    bottom: "-0.0625rem",
                    left: "var(--conv-rail-axis, 0.875rem)",
                    width: "0.125rem",
                    transform: "translateX(-50%)",
                  }}
                />

                <span
                  className={cn(
                    "relative min-w-0 flex-1 truncate text-[0.8125rem] font-normal",
                    isOpenInTab && "text-primary"
                  )}
                >
                  {formatConversationTitle(conversation.title) ||
                    t("untitledConversation")}
                </span>
              </button>

              {/* Expand/collapse affordance for delegation children. It overlays
                  the rail axis. A sibling of the row button (HTML forbids nested
                  buttons) with `stopPropagation` so a toggle never selects the
                  row; pointer events stay off until revealed so a click on the
                  rail area still selects the row when not hovering. */}
              {hasChildren && onToggleExpand && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation()
                    onToggleExpand(conversation.id)
                  }}
                  aria-label={
                    expanded ? t("collapseSubsessions") : t("expandSubsessions")
                  }
                  aria-expanded={expanded}
                  title={
                    expanded ? t("collapseSubsessions") : t("expandSubsessions")
                  }
                  className={cn(
                    "absolute top-0 bottom-0 z-20 flex items-center justify-center",
                    "cursor-pointer outline-none",
                    "opacity-0 pointer-events-none transition-opacity duration-150",
                    "group-hover:opacity-100 group-hover:pointer-events-auto",
                    "group-focus-within:opacity-100 group-focus-within:pointer-events-auto",
                    "[@media(hover:none)]:opacity-100 [@media(hover:none)]:pointer-events-auto"
                  )}
                  style={{
                    left: "var(--conv-rail-axis, 0.875rem)",
                    width: "0.875rem",
                    transform: "translateX(-50%)",
                  }}
                >
                  <ChevronRight
                    aria-hidden
                    className={cn(
                      "h-3 w-3 shrink-0 text-muted-foreground/70",
                      "transition-transform duration-200 ease-out",
                      expanded && "rotate-90"
                    )}
                  />
                </button>
              )}

              {SHOW_SIDEBAR_SESSION_USAGE && openContextKey ? (
                <SidebarConversationUsageChip
                  contextKey={openContextKey}
                  runtimeConversationId={runtimeConversationId}
                />
              ) : null}

              {/* Right slot: sizes to its content — the time / status badge
                  normally, the two quick-action buttons (pin, done) on hover —
                  so it never reserves more width than what is actually shown
                  (the title reflows slightly on hover). Meta and buttons swap via
                  `display` (group-hover:hidden / group-hover:flex), which also
                  drops the hidden buttons out of the tab order and a11y tree. The
                  buttons are siblings of the row button — never nested — so their
                  clicks don't select the conversation; `tabIndex={-1}` keeps them
                  mouse-only (the context menu Pin/Unpin + Status is the keyboard/
                  AT-accessible path). */}
              {/* pr-[0.375rem] + the list's px-1.5 (0.375rem) puts the time
                  badge / hover action buttons at a uniform 0.75rem inset from the
                  sidebar border — the same right edge as the section-header
                  actions, folder-header actions, and New chat / Search shortcut
                  badges. */}
              <div className="flex h-full shrink-0 items-center pr-[0.375rem]">
                <span
                  className={cn(
                    "flex items-center",
                    // Roots swap the badge out for the hover actions; sub-sessions
                    // have no actions, so keep the badge (incl. the running
                    // spinner) visible on hover.
                    !isSubsession && "group-hover:hidden"
                  )}
                >
                  {isRunning ? (
                    <span
                      className="relative inline-flex shrink-0 items-center justify-center"
                      title={tSidebar("statusRunningBadge")}
                    >
                      <Loader2
                        className="h-3.5 w-3.5 animate-spin text-amber-600 dark:text-amber-400"
                        aria-hidden
                      />
                      <span className="sr-only">
                        {tSidebar("statusRunningBadge")}
                      </span>
                    </span>
                  ) : isCancelled ? (
                    <span
                      className="relative inline-flex shrink-0 items-center justify-center"
                      title={tSidebar("statusCancelledBadge")}
                    >
                      <XCircle
                        className="h-3.5 w-3.5 text-destructive"
                        aria-hidden
                      />
                      <span className="sr-only">
                        {tSidebar("statusCancelledBadge")}
                      </span>
                    </span>
                  ) : timeLabel ? (
                    <span
                      className={cn(
                        "relative shrink-0 tabular-nums",
                        "text-[0.71875rem]",
                        isSelected
                          ? "font-medium text-muted-foreground"
                          : "font-normal text-muted-foreground/70"
                      )}
                    >
                      {timeLabel}
                    </span>
                  ) : null}
                </span>
                {/* Hover quick actions — roots only (sub-sessions opt out above).
                    Default /90 is the lightest muted shade that still clears the
                    3:1 non-text-contrast bar over the row's hover background; hover
                    deepens to full foreground. The folder ⋯ button shares this
                    exact palette so all action icons stay a consistent two colors.
                    Each button is justify-end so its 14px glyph flushes to the
                    slot's right edge (0.75rem) — the same edge the default
                    time/status badge fills — instead of sitting ~5px in as a
                    centred icon in a transparent box would. */}
                {!isSubsession && (
                  <div className="hidden items-center gap-px group-hover:flex">
                    {onTogglePin && (
                      <button
                        type="button"
                        tabIndex={-1}
                        onClick={(e) => {
                          e.stopPropagation()
                          onTogglePin(conversation.id, !isPinned)
                        }}
                        title={isPinned ? t("unpin") : t("pin")}
                        aria-label={isPinned ? t("unpin") : t("pin")}
                        className={cn(
                          "flex h-6 w-6 shrink-0 items-center justify-end rounded-[0.375rem]",
                          "cursor-pointer outline-none transition-colors duration-150",
                          "text-muted-foreground/90 hover:text-sidebar-foreground"
                        )}
                      >
                        {isPinned ? (
                          <PinOff className="h-[0.875rem] w-[0.875rem]" />
                        ) : (
                          <Pin className="h-[0.875rem] w-[0.875rem]" />
                        )}
                      </button>
                    )}
                    <button
                      type="button"
                      tabIndex={-1}
                      onClick={(e) => {
                        e.stopPropagation()
                        onStatusChange(
                          conversation.id,
                          isCompleted ? "in_progress" : "completed"
                        )
                      }}
                      title={isCompleted ? t("reopen") : t("markCompleted")}
                      aria-label={
                        isCompleted ? t("reopen") : t("markCompleted")
                      }
                      className={cn(
                        "flex h-6 w-6 shrink-0 items-center justify-end rounded-[0.375rem]",
                        "cursor-pointer outline-none transition-colors duration-150",
                        "text-muted-foreground/90 hover:text-sidebar-foreground"
                      )}
                    >
                      <CheckCircle2 className="h-[0.875rem] w-[0.875rem]" />
                    </button>
                  </div>
                )}
              </div>
            </div>
          </div>
        </ContextMenuTrigger>
        <ContextMenuContent>
          {onNewConversation && (
            <>
              <ContextMenuItem
                onSelect={() => onNewConversation(conversation.folder_id)}
              >
                <SquarePen className="h-4 w-4" />
                {t("newConversation")}
              </ContextMenuItem>
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuItem onSelect={handleRenameOpen}>
            <Pencil className="h-4 w-4" />
            {t("rename")}
          </ContextMenuItem>
          {onTogglePin && (
            <ContextMenuItem
              onSelect={() => onTogglePin(conversation.id, !isPinned)}
            >
              {isPinned ? (
                <PinOff className="h-4 w-4" />
              ) : (
                <Pin className="h-4 w-4" />
              )}
              {isPinned ? t("unpin") : t("pin")}
            </ContextMenuItem>
          )}
          <ContextMenuItem onSelect={() => setDetailsOpen(true)}>
            <Info className="h-4 w-4" />
            {tDetails("menuLabel")}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuSub>
            <ContextMenuSubTrigger>
              <Circle className="h-4 w-4" />
              {t("status")}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              {STATUS_ORDER.filter((s) => s !== conversation.status).map(
                (s) => (
                  <ContextMenuItem
                    key={s}
                    onSelect={() => onStatusChange(conversation.id, s)}
                  >
                    <ConversationStatusDot status={s} />
                    {tStatus(s)}
                  </ContextMenuItem>
                )
              )}
            </ContextMenuSubContent>
          </ContextMenuSub>
          <ContextMenuSeparator />
          <ContextMenuItem
            variant="destructive"
            onSelect={() => setDeleteOpen(true)}
          >
            <Trash2 className="h-4 w-4" />
            {t("delete")}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <Dialog open={renameOpen} onOpenChange={setRenameOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("renameConversation")}</DialogTitle>
          </DialogHeader>
          <Input
            value={renameValue}
            onChange={(e) => setRenameValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.nativeEvent.isComposing || e.key === "Process") return
              if (e.key === "Enter") handleRenameConfirm()
            }}
            autoFocus
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameOpen(false)}>
              {t("cancel")}
            </Button>
            <Button onClick={handleRenameConfirm}>{t("save")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteConversationTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("deleteConversationDescription", {
                title:
                  formatConversationTitle(conversation.title) ||
                  t("untitledConversation"),
              })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={handleDeleteConfirm}>
              {t("delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {detailsOpen && (
        <SessionDetailsDialog
          open
          onOpenChange={setDetailsOpen}
          summary={conversation}
        />
      )}
    </>
  )
})

function SidebarConversationUsageChip({
  contextKey,
  runtimeConversationId,
}: {
  contextKey: string
  runtimeConversationId?: number | null
}) {
  const sessionStats = useConversationRuntimeStore((s) =>
    runtimeConversationId != null
      ? (s.byConversationId.get(runtimeConversationId)?.sessionStats ?? null)
      : null
  )

  return (
    <SessionUsageChip
      contextKey={contextKey}
      sessionStats={sessionStats}
      popoverSide="right"
      showIcon={false}
      className="mr-1 shrink-0"
    />
  )
}
