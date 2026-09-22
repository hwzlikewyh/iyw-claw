"use client"

import { useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import {
  Check,
  FolderOpen,
  Loader2,
  ShieldAlert,
  ShieldCheck,
  X,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import type { PendingPermission } from "@/contexts/acp-connections-context"
import { parsePermissionToolCall } from "@/lib/permission-request"
import { resolvePermissionOptionLabel } from "@/lib/permission-option-label"
import { PermissionDetails } from "./permission-details"
import { permissionSummary } from "./permission-summary"

interface PermissionDialogProps {
  permission: PendingPermission | null
  onRespond: (requestId: string, optionId: string) => void | Promise<void>
}

export function PermissionDialog(props: PermissionDialogProps) {
  return props.permission ? (
    <PermissionCard
      key={props.permission.request_id}
      permission={props.permission}
      onRespond={props.onRespond}
    />
  ) : null
}

function PermissionCard({
  permission,
  onRespond,
}: PermissionDialogProps & { permission: PendingPermission }) {
  const t = useTranslations("Folder.chat.permissionDialog")
  const parsed = useMemo(
    () => parsePermissionToolCall(permission.tool_call),
    [permission.tool_call]
  )
  const response = usePermissionResponse({ permission, onRespond })
  return (
    <section
      aria-label={t("title")}
      className="mx-4 mb-3 min-w-0 shrink-0 overflow-hidden rounded-lg border border-border bg-card shadow-sm"
    >
      <PermissionHeader permission={permission} kind={parsed.normalizedKind} />
      <div className="max-h-[min(36vh,18rem)] min-w-0 space-y-3 overflow-y-auto overscroll-contain px-4 pb-4">
        <p className="text-sm font-medium [overflow-wrap:anywhere]">
          {parsed.command
            ? (permissionSummary(permission.tool_call) ?? t("commandRequest"))
            : parsed.title}
        </p>
        <PermissionDetails parsed={parsed} />
        {parsed.cwd && (
          <div className="flex items-start gap-2 text-xs leading-5 text-muted-foreground">
            <FolderOpen className="mt-0.5 size-3.5 shrink-0" />
            <span className="min-w-0 [overflow-wrap:anywhere]">
              {t("cwd", { cwd: parsed.cwd })}
            </span>
          </div>
        )}
      </div>
      <PermissionActions permission={permission} {...response} />
    </section>
  )
}

function usePermissionResponse({
  permission,
  onRespond,
}: PermissionDialogProps & { permission: PendingPermission }) {
  const [pending, setPending] = useState<string | null>(null)
  const [error, setError] = useState(false)
  const inFlight = useRef(false)
  const respond = async (optionId: string) => {
    if (inFlight.current) return
    inFlight.current = true
    setPending(optionId)
    setError(false)
    try {
      await onRespond(permission.request_id, optionId)
    } catch {
      setError(true)
    } finally {
      inFlight.current = false
      setPending(null)
    }
  }
  return { pending, error, respond }
}

function PermissionHeader({
  permission,
  kind,
}: {
  permission: PendingPermission
  kind: string
}) {
  const t = useTranslations("Folder.chat.permissionDialog")
  return (
    <header className="flex items-start gap-3 p-4">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-amber-500/10 text-amber-600 dark:text-amber-400">
        <ShieldAlert className="size-4.5" />
      </span>
      <div className="min-w-0 flex-1">
        <h3 className="text-sm font-semibold">{t("title")}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {t("subtitle")}
        </p>
      </div>
      <div className="flex max-w-[45%] flex-wrap justify-end gap-1.5">
        <Badge
          variant="outline"
          className="max-w-full rounded-md text-[10px] whitespace-normal [overflow-wrap:anywhere]"
        >
          {kind || t("kindFallbackTool")}
        </Badge>
        {Boolean(permission.queued) && (
          <span className="text-[10px] text-muted-foreground">
            {t("queuedCount", { count: permission.queued! })}
          </span>
        )}
      </div>
    </header>
  )
}

function PermissionActions({
  permission,
  pending,
  error,
  respond,
}: {
  permission: PendingPermission
  pending: string | null
  error: boolean
  respond: (id: string) => Promise<void>
}) {
  const t = useTranslations("Folder.chat.permissionDialog")
  const primaryId = permission.options.find(
    (option) => option.kind === "allow_once"
  )?.option_id
  const groups = [
    permission.options.filter((option) => option.kind.startsWith("reject")),
    permission.options.filter(
      (option) =>
        !option.kind.startsWith("reject") && option.option_id !== primaryId
    ),
    permission.options.filter((option) => option.option_id === primaryId),
  ]
  const button = (option: PendingPermission["options"][number]) => (
    <PermissionButton
      key={option.option_id}
      option={option}
      primary={option.option_id === primaryId}
      pending={pending}
      respond={respond}
    />
  )
  return (
    <footer className="border-t border-border/60 p-4">
      {error && (
        <p role="alert" className="mb-3 text-xs text-destructive">
          {t("submitError")}
        </p>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 max-w-full flex-wrap gap-2">
          {groups[0].map(button)}
        </div>
        <div className="ms-auto flex min-w-0 max-w-full flex-wrap justify-end gap-2">
          {[...groups[1], ...groups[2]].map(button)}
        </div>
      </div>
    </footer>
  )
}

function PermissionButton({
  option,
  primary,
  pending,
  respond,
}: {
  option: PendingPermission["options"][number]
  primary: boolean
  pending: string | null
  respond: (id: string) => Promise<void>
}) {
  const t = useTranslations("Folder.chat.permissionDialog")
  const reject = option.kind.startsWith("reject")
  const Icon = reject ? X : primary ? Check : ShieldCheck
  const label = resolvePermissionOptionLabel(option)
  return (
    <Button
      key={option.option_id}
      type="button"
      variant={reject ? "ghost" : primary ? "default" : "outline"}
      size="sm"
      disabled={pending !== null}
      className="h-auto min-h-9 min-w-0 max-w-full rounded-md py-2 text-xs whitespace-normal"
      onClick={() => void respond(option.option_id)}
    >
      {pending === option.option_id ? (
        <Loader2 className="size-3.5 animate-spin" />
      ) : (
        <Icon className="size-3.5" />
      )}
      <span className="min-w-0 [overflow-wrap:anywhere]">
        {label
          ? t(`options.${label.key}`, { command: label.command ?? "" })
          : option.name}
      </span>
    </Button>
  )
}
