"use client"

import { useTranslations } from "next-intl"
import { Loader2, WandSparkles } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import type { MemoryMigrationPreview } from "@/lib/user-memory-maintenance"

export function MemoryMigrationPreviewDialog({
  preview,
  busy,
  onReconcile,
  onClose,
}: {
  preview: MemoryMigrationPreview | null
  busy: boolean
  onReconcile: () => void
  onClose: () => void
}) {
  const t = useTranslations("UserMemorySettings.maintenance")
  return (
    <Dialog
      open={preview !== null}
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("preview")}</DialogTitle>
          <DialogDescription>{t("previewDescription")}</DialogDescription>
        </DialogHeader>
        {preview && (
          <PreviewContent
            preview={preview}
            busy={busy}
            onReconcile={onReconcile}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function PreviewContent({
  preview,
  busy,
  onReconcile,
}: {
  preview: MemoryMigrationPreview
  busy: boolean
  onReconcile: () => void
}) {
  const t = useTranslations("UserMemorySettings.maintenance")
  return (
    <div className="min-w-0 space-y-4">
      <div className="flex flex-wrap gap-3 text-xs">
        {Object.entries(preview.counts).map(([kind, count]) => (
          <span key={kind}>
            {kind}: {count}
          </span>
        ))}
      </div>
      {preview.warnings.length > 0 && (
        <PreviewWarnings warnings={preview.warnings} />
      )}
      {preview.unparsedLines.length > 0 && (
        <div className="space-y-2">
          <ul className="max-h-48 divide-y overflow-y-auto border-y text-xs">
            {preview.unparsedLines.map((line) => (
              <li key={line.contentDigest} className="space-y-1 py-2">
                <span className="text-muted-foreground">
                  {t("lineNumber", { line: line.lineNumber })}
                </span>
                <p className="whitespace-pre-wrap break-words">
                  {line.content ?? t("redacted")}
                </p>
              </li>
            ))}
          </ul>
          <Button disabled={busy} onClick={onReconcile}>
            {busy ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <WandSparkles className="size-4" />
            )}
            {t("convertLegacy")}
          </Button>
        </div>
      )}
      <ul className="divide-y">
        {preview.records.map((record) => (
          <li key={`${record.kind}:${record.id}`} className="space-y-1 py-3">
            <p className="whitespace-pre-wrap break-words text-sm">
              {record.content ?? t("redacted")}
            </p>
            <p className="break-all text-xs text-muted-foreground">
              {record.kind} · {record.state} · {record.scopeType}
            </p>
            {record.scopeKey && (
              <p className="break-all text-xs text-muted-foreground">
                {record.scopeKey}
              </p>
            )}
            <p className="break-all font-mono text-[11px] text-muted-foreground">
              {record.id}
            </p>
          </li>
        ))}
      </ul>
    </div>
  )
}

function PreviewWarnings({ warnings }: { warnings: string[] }) {
  const t = useTranslations("UserMemorySettings.maintenance")
  return (
    <ul className="space-y-1 text-xs text-amber-700 dark:text-amber-400">
      {warnings.map((warning) => (
        <li key={warning}>
          {warning.startsWith("unreadable:")
            ? `${t("unreadable")}: ${warning.slice("unreadable:".length)}`
            : t(
                warning as
                  | "authority_switch_not_implemented"
                  | "sensitive_content_redacted"
                  | "source_identity_conflict"
                  | "unparsed_memory_lines"
              )}
        </li>
      ))}
    </ul>
  )
}
