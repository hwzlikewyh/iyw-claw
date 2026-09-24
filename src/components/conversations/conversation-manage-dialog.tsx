"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import {
  Check,
  ChevronDown,
  Loader2,
  Pin,
  RefreshCw,
  Trash2,
} from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { STATUS_ORDER } from "@/lib/types"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
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
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  useConversationManager,
  type ConversationManager,
} from "./use-conversation-manager"
import { ConversationManagerFilters } from "./conversation-manager-filters"
import { ConversationManagerTable } from "./conversation-manager-table"

interface ConversationManageDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  folderId?: number
  folderName?: string
}

export function ConversationManageDialog(props: ConversationManageDialogProps) {
  const manager = useConversationManager(props)
  const t = useTranslations("SidebarDesign")
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const mobile = useIsMobile()
  const sidebar = useSidebarContext()
  return (
    <Dialog
      open={props.open}
      onOpenChange={(open) => {
        if (!manager.pending) props.onOpenChange(open)
      }}
    >
      <DialogContent
        className="flex h-[min(48rem,calc(100dvh-2rem))] max-w-[min(64rem,calc(100vw-2rem))] flex-col gap-4 overflow-hidden rounded-lg p-4 sm:p-6"
        onEscapeKeyDown={(event) => {
          if (manager.pending) event.preventDefault()
        }}
        onInteractOutside={(event) => {
          if (manager.pending) event.preventDefault()
        }}
      >
        <DialogHeader>
          <DialogTitle>
            {props.folderName
              ? tm("title", { name: props.folderName })
              : t("sessions")}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {t("sessions")}
          </DialogDescription>
        </DialogHeader>
        <ConversationManagerFilters
          manager={manager}
          scoped={props.folderId !== undefined}
        />
        <ManagerToolbar manager={manager} />
        {(manager.error || manager.incomplete) && (
          <div
            role="alert"
            className="flex items-center gap-2 text-xs text-destructive"
          >
            <span className="min-w-0 flex-1 break-words">
              {manager.error || t("incomplete")}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={manager.pending || manager.loading}
              onClick={manager.refresh}
            >
              {t("retry")}
            </Button>
          </div>
        )}
        <ConversationManagerTable
          manager={manager}
          onOpen={async (row) => {
            if (!(await manager.navigate(row))) return
            props.onOpenChange(false)
            if (mobile && sidebar.isOpen) sidebar.toggle()
          }}
        />
        <DialogFooter className="flex-row items-center justify-between gap-2 sm:justify-between">
          <span className="text-xs text-muted-foreground">
            {tm("matchedCount", { count: manager.rows.length })}
          </span>
          {manager.cursor && (
            <Button
              size="sm"
              variant="outline"
              disabled={manager.loading || manager.pending}
              onClick={manager.loadMore}
            >
              {manager.loading && <Loader2 className="size-3 animate-spin" />}
              {t("loadMore")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ManagerToolbar({ manager }: { manager: ConversationManager }) {
  const t = useTranslations("SidebarDesign")
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const ts = useTranslations("Folder.statusLabels")
  const [confirmDelete, setConfirmDelete] = useState(false)
  const disabled =
    manager.loading || manager.pending || !manager.selectedRows.length
  return (
    <div className="flex flex-wrap items-center gap-2">
      <span className="mr-auto text-xs text-muted-foreground">
        {tm("selectedCount", { count: manager.selectedRows.length })}
      </span>
      <Button
        size="sm"
        variant="ghost"
        disabled={disabled}
        onClick={() => void manager.run("pin")}
      >
        <Pin className="size-3.5" />
        {t("batchPin")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        disabled={disabled}
        onClick={() => void manager.run("complete")}
      >
        <Check className="size-3.5" />
        {t("batchComplete")}
      </Button>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="icon"
            variant="ghost"
            className="size-8"
            disabled={disabled}
            title={tm("setStatus")}
            aria-label={tm("setStatus")}
          >
            <ChevronDown className="size-3" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {STATUS_ORDER.map((status) => (
            <DropdownMenuItem
              key={status}
              onSelect={() => void manager.run(status)}
            >
              {ts(status)}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <Button
        size="sm"
        variant="ghost"
        className="text-destructive"
        disabled={disabled}
        onClick={() => setConfirmDelete(true)}
      >
        <Trash2 className="size-3.5" />
        {tm("deleteSelected")}
      </Button>
      <Button
        size="icon"
        variant="ghost"
        className="size-8"
        disabled={manager.pending || manager.loading}
        onClick={manager.refresh}
        title={t("refresh")}
        aria-label={t("refresh")}
      >
        <RefreshCw
          className={manager.loading ? "size-3.5 animate-spin" : "size-3.5"}
        />
      </Button>
      <ManagerDeleteConfirmation
        manager={manager}
        open={confirmDelete}
        onClose={() => setConfirmDelete(false)}
      />
    </div>
  )
}

function ManagerDeleteConfirmation({
  manager,
  open,
  onClose,
}: {
  manager: ConversationManager
  open: boolean
  onClose: () => void
}) {
  const tm = useTranslations("Folder.sidebar.manageConversations")
  const common = useTranslations("Folder.common")
  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !manager.pending) onClose()
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {tm("confirmDeleteTitle", { count: manager.selectedRows.length })}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {tm("confirmDeleteDescription")}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={manager.pending}>
            {common("cancel")}
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={manager.pending || !manager.selectedRows.length}
            onClick={(event) => {
              event.preventDefault()
              void manager.run("delete").then(onClose)
            }}
          >
            {manager.pending && <Loader2 className="size-3 animate-spin" />}
            {common("confirm")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
