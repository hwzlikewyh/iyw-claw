"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import {
  AlertCircle,
  Ban,
  ChevronDown,
  ChevronRight,
  Gauge,
  KeyRound,
  LogIn,
  Plus,
  RefreshCw,
  ServerCrash,
  WifiOff,
  X,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  knownSessionFailureActions,
  type SessionFailureAction,
} from "@/lib/session-failures"
import type { SessionFailureRecord } from "@/lib/types"
import { cn } from "@/lib/utils"
import { sanitizeAgentRuntimeErrorDetails } from "@/lib/agent-runtime-error"

const CATEGORY_ICONS: Record<string, typeof AlertCircle> = {
  connection: WifiOff,
  access: KeyRound,
  limit: Gauge,
  request: Ban,
  service: ServerCrash,
  unknown: AlertCircle,
}

const ACTION_ICONS: Record<SessionFailureAction, typeof RefreshCw> = {
  retry: RefreshCw,
  login: LogIn,
  new_session: Plus,
}

const ACTION_LABEL_KEYS = {
  retry: "action.retry",
  login: "action.login",
  new_session: "action.newSession",
} as const

const CATEGORY_LABEL_KEYS = {
  connection: "category.connection",
  access: "category.access",
  limit: "category.limit",
  request: "category.request",
  service: "category.service",
  unknown: "category.unknown",
} as const

type KnownCategory = keyof typeof CATEGORY_LABEL_KEYS

function knownCategory(category: string): KnownCategory {
  return Object.prototype.hasOwnProperty.call(CATEGORY_LABEL_KEYS, category)
    ? (category as KnownCategory)
    : "unknown"
}

export interface ActiveFailureStripProps {
  failure: SessionFailureRecord
  hiddenCount?: number
  dismissIds: string[]
  onAction?: (
    action: SessionFailureAction,
    failure: SessionFailureRecord
  ) => void
  onDismiss?: (ids: string[]) => void
}

export function ActiveFailureStrip({
  failure,
  hiddenCount = 0,
  dismissIds,
  onAction,
  onDismiss,
}: ActiveFailureStripProps) {
  const t = useTranslations("Folder.chat.sessionFailure")
  const [expanded, setExpanded] = useState(false)
  const warning = failure.severity === "warning"
  const category = knownCategory(failure.category)
  const Icon = CATEGORY_ICONS[category]
  const title = failure.title.trim() || t(CATEGORY_LABEL_KEYS[category])
  const details = failure.details?.trim()
    ? sanitizeAgentRuntimeErrorDetails(failure.details.trim())
    : null

  return (
    <div role="alert" className={failureStripClass(warning)}>
      <div className="flex flex-wrap items-center gap-2">
        <Icon aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
        <span
          className="min-w-0 basis-40 flex-1 truncate font-medium"
          title={details ?? title}
        >
          {title}
        </span>
        <FailureStripControls
          failure={failure}
          warning={warning}
          details={details}
          expanded={expanded}
          hiddenCount={hiddenCount}
          dismissIds={dismissIds}
          onAction={onAction}
          onDismiss={onDismiss}
          onToggleDetails={() => setExpanded((value) => !value)}
        />
      </div>
      {expanded && details && (
        <p className="mt-1.5 ps-[22px] whitespace-pre-wrap break-words text-[11px] opacity-80">
          {details}
        </p>
      )}
    </div>
  )
}

function failureStripClass(warning: boolean): string {
  return cn(
    "border-t px-4 py-2 text-xs",
    warning
      ? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
      : "border-destructive/20 bg-destructive/5 text-destructive"
  )
}

interface FailureStripControlsProps extends ActiveFailureStripProps {
  warning: boolean
  details: string | null
  expanded: boolean
  onToggleDetails: () => void
}

function FailureStripControls({
  failure,
  warning,
  details,
  expanded,
  hiddenCount = 0,
  dismissIds,
  onAction,
  onDismiss,
  onToggleDetails,
}: FailureStripControlsProps) {
  const t = useTranslations("Folder.chat.sessionFailure")
  const actionButtons =
    onAction && !warning
      ? knownSessionFailureActions(failure).map((action) => (
          <FailureActionButton
            key={action}
            action={action}
            failure={failure}
            onAction={onAction}
          />
        ))
      : null
  const DetailsChevron = expanded ? ChevronDown : ChevronRight
  return (
    <>
      {hiddenCount > 0 && (
        <span className="shrink-0 text-[10px] font-medium opacity-70">
          {t("moreIncidents", { count: hiddenCount })}
        </span>
      )}
      {actionButtons}
      {details && (
        <button
          type="button"
          aria-label={t("toggleDetails")}
          aria-expanded={expanded}
          className="shrink-0 rounded p-0.5 opacity-70 hover:opacity-100"
          onClick={onToggleDetails}
        >
          <DetailsChevron aria-hidden="true" className="h-3.5 w-3.5" />
        </button>
      )}
      {onDismiss && (
        <FailureDismissButton
          warning={warning}
          onClick={() => onDismiss(dismissIds)}
        />
      )}
    </>
  )
}

function FailureActionButton({
  action,
  failure,
  onAction,
}: {
  action: SessionFailureAction
  failure: SessionFailureRecord
  onAction: NonNullable<ActiveFailureStripProps["onAction"]>
}) {
  const t = useTranslations("Folder.chat.sessionFailure")
  const ActionIcon = ACTION_ICONS[action]
  return (
    <Button
      size="sm"
      variant="outline"
      className="h-6 shrink-0 px-2 text-xs"
      onClick={() => onAction(action, failure)}
    >
      <ActionIcon aria-hidden="true" className="me-1 h-3 w-3" />
      {t(ACTION_LABEL_KEYS[action])}
    </Button>
  )
}

function FailureDismissButton({
  warning,
  onClick,
}: {
  warning: boolean
  onClick: () => void
}) {
  const t = useTranslations("Folder.chat.sessionFailure")
  return (
    <Button
      size="icon"
      variant="ghost"
      className={cn(
        "h-6 w-6 shrink-0",
        warning
          ? "text-amber-700/70 hover:bg-amber-500/20 hover:text-amber-800 dark:text-amber-300/70 dark:hover:text-amber-200"
          : "text-destructive/70 hover:bg-destructive/10 hover:text-destructive"
      )}
      onClick={onClick}
      aria-label={t("dismiss")}
    >
      <X aria-hidden="true" className="h-3.5 w-3.5" />
    </Button>
  )
}
