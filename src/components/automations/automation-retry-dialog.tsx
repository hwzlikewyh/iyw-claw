"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import type { AutomationRun } from "@/lib/types"

export function AutomationRetryDialog({
  runs,
  batch,
  busy,
  onClose,
  onConfirm,
}: {
  runs: AutomationRun[]
  batch: boolean
  busy: boolean
  onClose: () => void
  onConfirm: (ids: number[]) => void
}) {
  const t = useTranslations("Automations")
  const [selected, setSelected] = useState(
    () => new Set(runs.map((run) => run.id))
  )
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>
            {t(batch ? "retryBatchTitle" : "retryTitle")}
          </DialogTitle>
          <DialogDescription>{t("retryDescription")}</DialogDescription>
        </DialogHeader>
        {batch && (
          <>
            <p className="text-xs text-muted-foreground">
              {t("retryBatchHint")}
            </p>
            <div className="max-h-64 overflow-y-auto rounded-lg border border-border">
              {runs.map((run) => (
                <label
                  key={run.id}
                  className="flex items-center gap-3 border-b border-border p-3 text-xs last:border-0"
                >
                  <input
                    type="checkbox"
                    checked={selected.has(run.id)}
                    aria-label={t("retrySelect", { id: run.id })}
                    onChange={(event) =>
                      setSelected((previous) => {
                        const next = new Set(previous)
                        if (event.target.checked) next.add(run.id)
                        else next.delete(run.id)
                        return next
                      })
                    }
                  />
                  {t("runNumber", { id: run.id })}
                  <span className="ml-auto text-muted-foreground">
                    {new Date(run.created_at).toLocaleString()}
                  </span>
                </label>
              ))}
            </div>
          </>
        )}
        {!batch && (
          <p className="rounded-lg bg-muted p-3 text-xs">
            {t("runNumber", { id: runs[0].id })}
          </p>
        )}
        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button
            disabled={busy || selected.size === 0}
            onClick={() => onConfirm([...selected])}
          >
            {t("retryConfirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
