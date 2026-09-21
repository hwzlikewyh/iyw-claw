"use client"

import { useTranslations } from "next-intl"
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
  onClose,
}: {
  preview: MemoryMigrationPreview | null
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
        {preview && <PreviewContent preview={preview} />}
      </DialogContent>
    </Dialog>
  )
}

function PreviewContent({ preview }: { preview: MemoryMigrationPreview }) {
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
