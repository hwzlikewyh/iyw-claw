"use client"

import { useRef } from "react"
import { Upload } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { Transport } from "@/lib/transport"
import {
  LIBRARY_BATCH_LIMIT,
  type LibraryTarget,
} from "@/lib/knowledge-library"
import { LIBRARY_ACCEPT } from "@/lib/library-file-rules"
import { LibraryUploadRow } from "./library-upload-row"
import { useLibraryUploads } from "./use-library-uploads"

interface UploadDialogProps {
  transport: Transport
  target: LibraryTarget
  onChanged: () => void
  onClose: () => void
}
type UploadQueue = ReturnType<typeof useLibraryUploads>

export function LibraryUploadDialog(props: UploadDialogProps) {
  const t = useTranslations("Library")
  const queue = useLibraryUploads({
    ...props,
    ruleMessage: (failure) => t(failure.key, { limit: failure.limit ?? 0 }),
  })
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose()
      }}
    >
      <DialogContent
        className="max-w-2xl gap-4 rounded-lg p-4 sm:max-w-2xl sm:p-6"
        onInteractOutside={(event) => {
          if (queue.busy) event.preventDefault()
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("uploadTitle")}</DialogTitle>
          <DialogDescription>
            {t("uploadTarget", {
              category: t(
                props.target.category === 0 ? "knowledge" : "customers"
              ),
              folder: props.target.folderName,
            })}
          </DialogDescription>
        </DialogHeader>
        <UploadDropzone
          disabled={queue.busy || queue.validating}
          onFiles={queue.addFiles}
        />
        <UploadQueueContent queue={queue} />
        <p className="text-xs leading-5 text-muted-foreground">
          {t("uploadHint")}
        </p>
        <UploadActions queue={queue} onClose={props.onClose} />
      </DialogContent>
    </Dialog>
  )
}

function UploadQueueContent({ queue }: { queue: UploadQueue }) {
  const t = useTranslations("Library")
  return (
    <>
      <div className="flex items-center justify-between text-xs text-muted-foreground">
        <span>
          {t("selectedCount", {
            count: queue.tasks.length,
            limit: LIBRARY_BATCH_LIMIT,
          })}
        </span>
        {queue.validating && <span role="status">{t("validating")}</span>}
      </div>
      {queue.tasks.length > 0 && (
        <ul
          className="max-h-[min(21rem,40dvh)] overflow-y-auto pr-1"
          aria-label={t("uploadQueue")}
        >
          {queue.tasks.map((task) => (
            <LibraryUploadRow
              key={task.id}
              task={task}
              busy={queue.busy}
              onCancel={() => queue.cancel(task.id)}
              onRemove={() => queue.remove(task.id)}
            />
          ))}
        </ul>
      )}
      {queue.error && (
        <p role="alert" className="text-sm text-destructive">
          {queue.error}
        </p>
      )}
      {queue.tasks.some((task) => task.phase === "cancelled") && (
        <p className="text-xs text-muted-foreground">{t("cancelNotice")}</p>
      )}
    </>
  )
}

interface DropzoneProps {
  disabled: boolean
  onFiles: (files: File[]) => Promise<void>
}

function UploadDropzone(props: DropzoneProps) {
  const t = useTranslations("Library")
  const input = useRef<HTMLInputElement>(null)
  return (
    <div className="space-y-3">
      <input
        ref={input}
        type="file"
        multiple
        accept={LIBRARY_ACCEPT}
        disabled={props.disabled}
        className="sr-only"
        tabIndex={-1}
        aria-label={t("chooseFiles")}
        onChange={(event) => {
          void props.onFiles(Array.from(event.target.files ?? []))
          event.target.value = ""
        }}
      />
      <UploadDropButton {...props} onClick={() => input.current?.click()} />
      <details className="text-xs text-muted-foreground">
        <summary className="w-fit cursor-pointer rounded py-1 focus-visible:outline focus-visible:outline-ring">
          {t("formatLimits")}
        </summary>
        <p className="mt-2 whitespace-pre-line leading-5">
          {t("formatLimitDetails")}
        </p>
      </details>
    </div>
  )
}

function UploadDropButton({
  disabled,
  onFiles,
  onClick,
}: DropzoneProps & { onClick: () => void }) {
  const t = useTranslations("Library")
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault()
        if (!disabled) void onFiles(Array.from(event.dataTransfer.files))
      }}
      className="flex w-full flex-col items-center gap-2 rounded-md border border-dashed px-4 py-6 text-center hover:bg-muted/50 focus-visible:outline focus-visible:outline-ring disabled:cursor-not-allowed disabled:text-muted-foreground"
    >
      <Upload
        className="mb-1 size-5 text-muted-foreground"
        aria-hidden="true"
      />
      <span className="text-sm font-medium">{t("dropFiles")}</span>
      <span className="text-xs text-muted-foreground">
        {t("uploadFormats")}
      </span>
    </button>
  )
}

function UploadActions({
  queue,
  onClose,
}: {
  queue: UploadQueue
  onClose: () => void
}) {
  const t = useTranslations("Library")
  const eligible = queue.tasks.some(
    (task) => task.phase === "ready" || task.phase === "failed"
  )
  const action = queue.busy
    ? "uploading"
    : queue.tasks.some((task) => task.phase === "failed")
      ? "retryUploads"
      : "startUpload"
  return (
    <footer className="flex flex-wrap items-center justify-between gap-3 border-t pt-4">
      <Button
        variant="ghost"
        size="sm"
        disabled={queue.busy || queue.validating || !queue.tasks.length}
        onClick={queue.clear}
      >
        {t("clearQueue")}
      </Button>
      <div className="flex items-center gap-2">
        <Button variant="outline" onClick={onClose}>
          {t(queue.busy ? "cancelAndClose" : "close")}
        </Button>
        <Button
          disabled={!eligible || queue.busy || queue.validating}
          onClick={() => void queue.start()}
          className="bg-foreground text-background hover:bg-foreground/90"
        >
          {t(action)}
        </Button>
      </div>
    </footer>
  )
}
