"use client"

import { Check, Loader2, X } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { libraryFileSize } from "@/lib/knowledge-library"
import { cn } from "@/lib/utils"
import type { UploadTask } from "./use-library-uploads"

interface UploadRowProps {
  task: UploadTask
  busy: boolean
  onCancel: () => void
  onRemove: () => void
}

export function LibraryUploadRow(props: UploadRowProps) {
  const { task } = props
  return (
    <li className="min-w-0 space-y-2 border-t py-3">
      <UploadSummary {...props} />
      {(task.phase === "receiving" || task.phase === "uploading") && (
        <UploadProgress task={task} />
      )}
      {task.error && (
        <p className="break-words text-xs text-destructive" role="alert">
          {task.error}
        </p>
      )}
    </li>
  )
}

function UploadSummary(props: UploadRowProps) {
  const { task } = props
  const locale = useLocale()
  return (
    <div className="flex min-w-0 items-center gap-3">
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm" title={task.file.name}>
          {task.file.name}
        </p>
        <p className="mt-1 text-xs tabular-nums text-muted-foreground">
          {libraryFileSize(task.file.size, locale)}
        </p>
      </div>
      <UploadPhase task={task} />
      <UploadRowActions {...props} />
    </div>
  )
}

function UploadPhase({ task }: { task: UploadTask }) {
  const t = useTranslations("Library")
  const running =
    task.phase === "receiving" ||
    task.phase === "uploading" ||
    task.phase === "registering"
  const failed = task.phase === "invalid" || task.phase === "failed"
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1.5 text-xs",
        failed ? "text-destructive" : "text-muted-foreground"
      )}
      aria-live="polite"
    >
      {running && (
        <Loader2 className="size-3 animate-spin" aria-hidden="true" />
      )}
      {task.phase === "complete" && (
        <Check className="size-3.5" aria-hidden="true" />
      )}
      {t(`uploadPhase.${task.phase}`)}
    </span>
  )
}

function UploadRowActions({ task, busy, onCancel, onRemove }: UploadRowProps) {
  const t = useTranslations("Library")
  const cancellable =
    busy &&
    ["ready", "receiving", "uploading", "registering"].includes(task.phase)
  return (
    <>
      {cancellable && (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={onCancel}
          aria-label={t("cancelFile", { name: task.file.name })}
        >
          <X className="size-3.5" />
        </Button>
      )}
      {!busy && (
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={onRemove}
          aria-label={t("removeFile", { name: task.file.name })}
        >
          <X className="size-3.5" />
        </Button>
      )}
    </>
  )
}

function UploadProgress({ task }: { task: UploadTask }) {
  const t = useTranslations("Library")
  const bytes = task.phase === "receiving" ? task.receivedBytes : task.sentBytes
  const progress = Math.max(
    0,
    Math.min(100, Math.round((bytes / task.file.size) * 100))
  )
  return (
    <div className="flex items-center gap-3">
      <div
        role="progressbar"
        aria-label={t(`uploadPhase.${task.phase}`)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={progress}
        className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted"
      >
        <div
          className="h-full bg-foreground transition-[width] motion-reduce:transition-none"
          style={{ width: progress + "%" }}
        />
      </div>
      <span className="w-10 text-right text-xs tabular-nums text-muted-foreground">
        {progress}%
      </span>
    </div>
  )
}
