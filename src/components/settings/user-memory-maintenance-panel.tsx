"use client"

import { useState } from "react"
import { Check, Loader2, RefreshCw, ScanEye, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { MemoryReview } from "@/lib/user-memory-maintenance"
import { useMemoryMaintenance } from "./use-memory-maintenance"
import { MemoryMigrationPreviewDialog } from "./user-memory-migration-preview"

type Maintenance = ReturnType<typeof useMemoryMaintenance>

export function UserMemoryMaintenancePanel({
  onUpdated,
}: {
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.maintenance")
  const state = useMemoryMaintenance(onUpdated)
  return (
    <section className="space-y-3 border-y py-4">
      <MaintenanceHeader state={state} />
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
      {state.status ? (
        <MaintenanceContent state={state} />
      ) : (
        <p role="status" className="text-xs text-muted-foreground">
          {t("loading")}
        </p>
      )}
      <MemoryMigrationPreviewDialog
        preview={state.preview}
        busy={state.busy}
        onReconcile={() => void state.reconcilePreview()}
        onClose={() => state.setPreview(null)}
      />
    </section>
  )
}

function MaintenanceHeader({ state }: { state: Maintenance }) {
  const t = useTranslations("UserMemorySettings.maintenance")
  return (
    <div className="flex items-center justify-between gap-3">
      <h2 className="text-sm font-medium">{t("title")}</h2>
      <div className="flex gap-1">
        <Button
          variant="ghost"
          size="icon"
          title={t("preview")}
          aria-label={t("preview")}
          disabled={state.busy}
          onClick={() => void state.loadPreview()}
        >
          <ScanEye className="size-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title={t("run")}
          aria-label={t("run")}
          disabled={state.busy || state.status?.busy}
          onClick={() => void state.run()}
        >
          {state.busy || state.status?.busy ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
        </Button>
      </div>
    </div>
  )
}

function MaintenanceContent({ state }: { state: Maintenance }) {
  const t = useTranslations("UserMemorySettings.maintenance")
  const [history, setHistory] = useState(false)
  const status = state.status!
  const reviews = status.modelReview.reviews.filter(
    (review) => history || review.status === "pending"
  )
  return (
    <>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
        <span>
          {t("active")}: {status.activeCount}
        </span>
        <span>
          {t("expired")}: {status.expiredCount}
        </span>
        <span>
          {t("lastReview")}:{" "}
          {status.modelReview.lastCompletedAt
            ? new Date(status.modelReview.lastCompletedAt).toLocaleString()
            : t("never")}
        </span>
      </div>
      {status.modelReview.lastErrorCode && (
        <p role="alert" className="text-xs text-destructive">
          {t("reviewFailed")}
        </p>
      )}
      <label className="flex items-center gap-2 text-xs">
        <input
          type="checkbox"
          checked={history}
          onChange={(event) => setHistory(event.target.checked)}
        />
        {t("history")}
      </label>
      {reviews.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t("empty")}</p>
      ) : (
        <ul className="max-h-96 divide-y overflow-y-auto">
          {reviews.map((review) => (
            <ReviewRow key={review.id} review={review} state={state} />
          ))}
        </ul>
      )}
    </>
  )
}

function ReviewRow({
  review,
  state,
}: {
  review: MemoryReview
  state: Maintenance
}) {
  const t = useTranslations("UserMemorySettings.maintenance")
  const stale = state.status!.staleReviewIds.includes(review.id)
  return (
    <li className="space-y-2 py-3">
      <p className="whitespace-pre-wrap break-words text-sm">
        {review.content}
      </p>
      <blockquote className="border-l-2 pl-3 text-xs text-muted-foreground">
        {review.quote}
      </blockquote>
      <p className="break-words text-xs">{review.reason}</p>
      <div className="flex items-center justify-between gap-3 text-xs">
        <span>
          {t(stale && review.status === "pending" ? "stale" : review.status)}
        </span>
        {review.status === "pending" && (
          <div className="flex gap-1">
            <Button
              size="icon"
              variant="ghost"
              title={t("apply")}
              aria-label={t("apply")}
              disabled={state.busy || stale}
              onClick={() => void state.resolve(review.id, true)}
            >
              <Check className="size-4" />
            </Button>
            <Button
              size="icon"
              variant="ghost"
              title={t("dismiss")}
              aria-label={t("dismiss")}
              disabled={state.busy}
              onClick={() => void state.resolve(review.id, false)}
            >
              <X className="size-4" />
            </Button>
          </div>
        )}
      </div>
    </li>
  )
}
