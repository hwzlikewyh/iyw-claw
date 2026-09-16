"use client"

import { Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { SessionIdentity, SessionMetadata } from "./session-details-identity"
import {
  SessionContextUsage,
  SessionTokenBreakdown,
  SessionUsageOverview,
} from "./session-details-usage"
import {
  useSessionDetails,
  type SessionDetailsDialogProps,
} from "./session-details-data"

export { resolveSessionDurationMs } from "./session-details-data"

export function SessionDetailsDialog(props: SessionDetailsDialogProps) {
  const t = useTranslations("Folder.sessionDetails")
  const refreshT = useTranslations("UsageSettings")
  const data = useSessionDetails(props)
  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-w-xl gap-4 rounded-lg p-5 sm:p-6">
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription className="sr-only">
            {t("subtitle")}
          </DialogDescription>
        </DialogHeader>
        <div className="min-w-0">
          <SessionIdentity summary={data.summary} model={data.model} />
          {data.loading && (
            <div
              role="status"
              className="flex items-center gap-2 pt-4 text-xs text-muted-foreground"
            >
              <Loader2 className="size-3.5 animate-spin" />
              {t("loadingStats")}
            </div>
          )}
          {data.error && (
            <div
              role="alert"
              className="flex items-center justify-between gap-3 pt-4 text-xs text-destructive"
            >
              <span>{t("loadFailed")}</span>
              <button
                type="button"
                onClick={data.retry}
                title={refreshT("refresh")}
                aria-label={refreshT("refresh")}
                className="inline-flex size-7 items-center justify-center rounded hover:bg-muted"
              >
                <RefreshCw className="size-3.5" />
              </button>
            </div>
          )}
          <SessionUsageOverview summary={data.summary} stats={data.stats} />
          <SessionTokenBreakdown stats={data.stats} />
          <SessionContextUsage stats={data.stats} />
          <SessionMetadata summary={data.summary} />
        </div>
      </DialogContent>
    </Dialog>
  )
}
