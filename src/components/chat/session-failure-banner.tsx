"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"
import { CheckCircle2, X } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ActiveFailureStrip } from "@/components/chat/session-failure-active-strip"
import {
  activeSessionFailureView,
  mostRecentRecoveredWarning,
  type SessionFailureAction,
} from "@/lib/session-failures"
import type { SessionFailureRecord } from "@/lib/types"

const RECOVERED_VISIBLE_MS = 10_000

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

interface SessionFailureBannerProps {
  failures: SessionFailureRecord[]
  onAction?: (
    action: SessionFailureAction,
    failure: SessionFailureRecord
  ) => void
  onDismiss?: (ids: string[]) => void
}

export function SessionFailureBanner({
  failures,
  onAction,
  onDismiss,
}: SessionFailureBannerProps) {
  const { errors, warning, hiddenWarnings, warningIds } =
    activeSessionFailureView(failures)
  const recovered = mostRecentRecoveredWarning(failures)
  const hasActive = errors.length > 0 || warning !== null

  if (!hasActive && !recovered) return null

  return (
    <>
      {errors.map((failure) => (
        <ActiveFailureStrip
          key={failure.id}
          failure={failure}
          dismissIds={[failure.id]}
          onAction={onAction}
          onDismiss={onDismiss}
        />
      ))}
      {warning && (
        <ActiveFailureStrip
          key={warning.id}
          failure={warning}
          hiddenCount={hiddenWarnings}
          dismissIds={warningIds}
          onAction={onAction}
          onDismiss={onDismiss}
        />
      )}
      {!hasActive && recovered && (
        <RecoveredStrip
          key={`${recovered.id}@${recovered.revision}`}
          failure={recovered}
          onDismiss={onDismiss}
        />
      )}
    </>
  )
}

function RecoveredStrip({
  failure,
  onDismiss,
}: {
  failure: SessionFailureRecord
  onDismiss?: SessionFailureBannerProps["onDismiss"]
}) {
  const t = useTranslations("Folder.chat.sessionFailure")
  const title =
    failure.title.trim() ||
    t(CATEGORY_LABEL_KEYS[knownCategory(failure.category)])
  const id = failure.id

  useEffect(() => {
    if (!onDismiss) return
    const timer = setTimeout(() => onDismiss([id]), RECOVERED_VISIBLE_MS)
    return () => clearTimeout(timer)
  }, [id, onDismiss])

  return (
    <div className="border-t border-border/50 bg-muted/30 px-4 py-1.5 text-[11px] text-muted-foreground">
      <div className="flex items-center gap-2">
        <CheckCircle2 aria-hidden="true" className="h-3 w-3 shrink-0" />
        <span className="min-w-0 flex-1 truncate">
          {t("recovered")} · {title}
        </span>
        {onDismiss && (
          <Button
            size="icon"
            variant="ghost"
            className="h-5 w-5 shrink-0 text-muted-foreground/70 hover:text-foreground"
            onClick={() => onDismiss([id])}
            aria-label={t("dismiss")}
          >
            <X aria-hidden="true" className="h-3 w-3" />
          </Button>
        )}
      </div>
    </div>
  )
}
