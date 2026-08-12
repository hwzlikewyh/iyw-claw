"use client"

import { useCallback, type PointerEvent } from "react"
import { Reorder, useDragControls } from "motion/react"
import {
  CircleAlert,
  GripVertical,
  LoaderCircle,
  LockKeyhole,
  MoreHorizontal,
  RotateCcw,
  ShieldAlert,
  X,
} from "lucide-react"
import { useTranslations } from "next-intl"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

import { PlainTextWithBadges } from "@/components/message/plain-text-with-badges"
import { UserImageAttachments } from "@/components/message/user-image-attachments"
import { UserResourceLinks } from "@/components/message/user-resource-links"
import {
  extractUserImagesFromDraft,
  extractUserResourcesFromDraft,
} from "@/lib/prompt-draft"
import type { AgentInputItem, PromptDraft } from "@/lib/types"

export interface AgentInputWaitingRowProps {
  item: AgentInputItem
  index: number
  visible: AgentInputItem[]
  onDelete?: (id: string) => void
  onRetry?: (id: string) => void
  onForceThrough?: (messageId: string, expectedPrefixIds: string[]) => void
  onReorderFinished?: () => void
  reorderDisabled: boolean
  forceDisabled: boolean
}

export function isAgentInputLocked(item: AgentInputItem): boolean {
  return (
    item.status === "dispatching" ||
    item.force_batch_id != null ||
    item.force_requested_at != null
  )
}

function StatusIcon({ failed, locked }: { failed: boolean; locked: boolean }) {
  if (failed) {
    return <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-destructive" />
  }
  if (locked) {
    return (
      <LockKeyhole className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
    )
  }
  return (
    <LoaderCircle className="mt-0.5 size-3.5 shrink-0 animate-spin text-muted-foreground" />
  )
}

function WaitingMessage({ item }: { item: AgentInputItem }) {
  const t = useTranslations("Folder.chat.agentInput")
  const draft: PromptDraft = {
    blocks: item.payload.blocks,
    displayText: item.payload.display_text,
  }
  const images = extractUserImagesFromDraft(draft)
  const resources = extractUserResourcesFromDraft(draft)
  const failed = item.status === "failed"
  const locked = isAgentInputLocked(item)
  return (
    <div className="min-w-0 flex-1 space-y-1">
      <div className="text-[10px] font-medium text-muted-foreground">
        {failed ? t("failed") : locked ? t("dispatching") : t("waiting")}
      </div>
      {images.length > 0 && <UserImageAttachments images={images} />}
      {item.payload.display_text.trim() && (
        <PlainTextWithBadges
          text={item.payload.display_text}
          className="line-clamp-3 text-xs text-foreground/85"
        />
      )}
      {!item.payload.display_text.trim() && resources.length > 0 && (
        <UserResourceLinks resources={resources} />
      )}
    </div>
  )
}

function WaitingActions(props: AgentInputWaitingRowProps) {
  const { item, index, visible, onDelete, onRetry, onForceThrough } = props
  const t = useTranslations("Folder.chat.agentInput")
  const failed = item.status === "failed"
  const locked = isAgentInputLocked(item)
  const prefix = visible.slice(0, index + 1)
  const cannotForce =
    props.forceDisabled ||
    locked ||
    prefix.some((candidate) => candidate.status === "failed")
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="shrink-0 rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          title={t("actions")}
          aria-label={t("actions")}
        >
          <MoreHorizontal className="size-3.5" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="top">
        {onForceThrough && (
          <DropdownMenuItem
            disabled={cannotForce}
            onSelect={() =>
              onForceThrough(
                item.id,
                prefix.map((candidate) => candidate.id)
              )
            }
          >
            <ShieldAlert className="mr-2 size-3.5" />
            {t("safeForce")}
          </DropdownMenuItem>
        )}
        {failed && onRetry && (
          <DropdownMenuItem onSelect={() => onRetry(item.id)}>
            <RotateCcw className="mr-2 size-3.5" />
            {t("retry")}
          </DropdownMenuItem>
        )}
        {item.status === "waiting" && !locked && onDelete && (
          <DropdownMenuItem onSelect={() => onDelete(item.id)}>
            <X className="mr-2 size-3.5" />
            {t("delete")}
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function AgentInputWaitingRow(props: AgentInputWaitingRowProps) {
  const { item, onReorderFinished, reorderDisabled } = props
  const t = useTranslations("Folder.chat.agentInput")
  const dragControls = useDragControls()
  const locked = isAgentInputLocked(item)
  const startDrag = useCallback(
    (event: PointerEvent<HTMLButtonElement>) => {
      event.preventDefault()
      event.stopPropagation()
      dragControls.start(event)
    },
    [dragControls]
  )
  const content = (
    <>
      <button
        type="button"
        disabled={locked || reorderDisabled}
        onPointerDown={startDrag}
        className="mt-0.5 shrink-0 cursor-grab touch-none p-0 text-muted-foreground/60 hover:text-foreground active:cursor-grabbing disabled:cursor-not-allowed disabled:opacity-40"
        aria-label={t("reorder")}
      >
        <GripVertical className="size-3.5" />
      </button>
      <StatusIcon failed={item.status === "failed"} locked={locked} />
      <WaitingMessage item={item} />
      <WaitingActions {...props} />
    </>
  )
  const className =
    "flex min-w-0 items-start gap-2 rounded-md border border-border/70 bg-muted/35 px-2 py-1.5"
  if (locked) return <div className={className}>{content}</div>
  return (
    <Reorder.Item
      as="div"
      value={item}
      dragListener={false}
      dragControls={dragControls}
      onDragEnd={onReorderFinished}
      className={className}
    >
      {content}
    </Reorder.Item>
  )
}
