"use client"

import { useState } from "react"
import { Check, FileDiff } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  resolveMemoryFile,
  type ConflictAction,
  type MemoryFileConflict,
} from "@/lib/user-memory-reconcile"
import type { useMemoryReconciliation } from "./use-memory-reconciliation"

type State = ReturnType<typeof useMemoryReconciliation>

export function MemoryConflictFile({
  file,
  state,
}: {
  file: MemoryFileConflict
  state: State
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const [open, setOpen] = useState(false)
  return (
    <>
      <div className="flex items-center justify-between gap-2 border-b py-2">
        <span className="min-w-0 break-all text-xs">{file.name}</span>
        <Button
          size="sm"
          variant="outline"
          disabled={state.busy}
          onClick={() => setOpen(true)}
        >
          <FileDiff className="size-4" />
          {t("compare")}
        </Button>
      </div>
      <Dialog
        open={open}
        onOpenChange={(value) => !state.busy && setOpen(value)}
      >
        <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>{t("compare")}</DialogTitle>
            <DialogDescription className="break-all">
              {file.name}
            </DialogDescription>
          </DialogHeader>
          <FileComparison file={file} />
          <FileResolution file={file} state={state} />
          {state.error && (
            <p role="alert" className="break-words text-xs text-destructive">
              {state.error}
            </p>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}

function FileComparison({ file }: { file: MemoryFileConflict }) {
  const t = useTranslations("UserMemorySettings.reconcile")
  if (file.redacted)
    return <p className="text-sm text-muted-foreground">{t("redacted")}</p>
  return (
    <div className="grid min-w-0 gap-3 sm:grid-cols-2">
      <FileContent label={t("current")} content={file.current} />
      <FileContent label={t("external")} content={file.external} />
    </div>
  )
}

function FileContent({
  label,
  content,
}: {
  label: string
  content: string | null
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  return (
    <div className="min-w-0 space-y-2">
      <h3 className="text-sm font-medium">{label}</h3>
      <pre className="max-h-64 overflow-y-auto whitespace-pre-wrap break-all rounded-md border p-3 text-xs leading-5">
        {content ?? t("missing")}
      </pre>
    </div>
  )
}

function FileResolution({
  file,
  state,
}: {
  file: MemoryFileConflict
  state: State
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const [action, setAction] = useState<ConflictAction>("keep_current")
  const apply = () =>
    state.apply(() =>
      resolveMemoryFile({
        expectedRevision: state.preview!.revision,
        name: file.name,
        action,
      })
    )
  return (
    <div className="space-y-3 border-t pt-3">
      <ResolutionChoices
        file={file}
        busy={state.busy}
        action={action}
        onAction={setAction}
      />
      {!file.importAllowed && (
        <p className="text-xs text-muted-foreground">{t("dedicatedEdit")}</p>
      )}
      <Button
        size="sm"
        disabled={
          state.busy ||
          !state.preview ||
          (action === "import_paused" && !file.importAllowed)
        }
        onClick={() => void apply()}
      >
        <Check className="size-4" />
        {t("apply")}
      </Button>
    </div>
  )
}

function ResolutionChoices({
  file,
  busy,
  action,
  onAction,
}: {
  file: MemoryFileConflict
  busy: boolean
  action: ConflictAction
  onAction: (value: ConflictAction) => void
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  return (
    <fieldset disabled={busy} className="space-y-2 text-sm">
      <legend className="mb-2 font-medium">{t("resolution")}</legend>
      <label className="flex items-start gap-2">
        <input
          type="radio"
          name={`resolve-${file.name}`}
          checked={action === "keep_current"}
          onChange={() => onAction("keep_current")}
        />
        {t("keepCurrent")}
      </label>
      <label className="flex items-start gap-2">
        <input
          type="radio"
          name={`resolve-${file.name}`}
          checked={action === "import_paused"}
          disabled={!file.importAllowed}
          onChange={() => onAction("import_paused")}
        />
        {t("importPaused")}
      </label>
    </fieldset>
  )
}
