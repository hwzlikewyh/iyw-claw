"use client"

import { useId } from "react"
import { Loader2, MessageSquare, RefreshCw, Upload } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { useNativeShareInbox } from "./use-native-share-inbox"

type InboxModel = ReturnType<typeof useNativeShareInbox>

export function NativeShareInbox() {
  const model = useNativeShareInbox()
  return (
    <Dialog
      open={model.open}
      onOpenChange={(open) => {
        if (!model.busy) model.setOpen(open)
      }}
    >
      <DialogContent className="max-w-3xl gap-4 rounded-2xl p-5">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-base">
            <MessageSquare className="size-4" />
            {model.t("title")}
          </DialogTitle>
          <DialogDescription>{model.t("description")}</DialogDescription>
        </DialogHeader>
        <InboxToolbar model={model} />
        {model.error && (
          <p
            role="alert"
            className="rounded-md border border-destructive/30 p-3 text-sm text-destructive"
          >
            {model.error}
          </p>
        )}
        <div className="grid min-h-0 gap-4 md:grid-cols-[13rem_minmax(0,1fr)]">
          <BatchList model={model} />
          <BatchPreview model={model} />
        </div>
        <ConversationTarget model={model} />
      </DialogContent>
    </Dialog>
  )
}

function InboxToolbar({ model }: { model: InboxModel }) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button
        size="sm"
        variant="outline"
        disabled={model.busy}
        onClick={() => void model.manualImport()}
      >
        <Upload className="size-3.5" />
        {model.t("importFile")}
      </Button>
      <Button
        size="sm"
        variant="ghost"
        disabled={model.busy}
        onClick={() => void model.refresh()}
      >
        <RefreshCw className="size-3.5" />
        {model.t("refresh")}
      </Button>
      {model.status?.canRegister && !model.status.installed && (
        <Button
          size="sm"
          variant="outline"
          disabled={model.busy}
          onClick={() => void model.register()}
        >
          {model.t("enableSharing")}
        </Button>
      )}
      <p className="text-xs text-muted-foreground">
        {model.status
          ? model.t(model.status.installed ? "installed" : "notInstalled")
          : ""}
      </p>
    </div>
  )
}

function BatchList({ model }: { model: InboxModel }) {
  return (
    <div
      className="max-h-72 overflow-y-auto rounded-lg border md:max-h-96"
      aria-label={model.t("batches")}
    >
      {model.items.length === 0 && (
        <p className="p-4 text-sm text-muted-foreground">{model.t("empty")}</p>
      )}
      {model.items.map((batch) => (
        <button
          key={batch.id}
          type="button"
          disabled={model.busy}
          aria-pressed={batch.id === model.selected}
          onClick={() => model.setSelected(batch.id)}
          className={cn(
            "block w-full border-b p-3 text-left last:border-0 hover:bg-muted/50 focus-visible:outline focus-visible:outline-ring",
            batch.id === model.selected && "bg-muted"
          )}
        >
          <span className="block truncate text-sm">
            {batch.items[0]?.name || model.t("archiveFiles")}
          </span>
          <span className="mt-1 block text-xs text-muted-foreground">
            {new Date(batch.createdAt).toLocaleString()}
          </span>
          <span className="mt-1 block text-xs text-muted-foreground">
            {batch.error || model.t(batch.imported ? "imported" : "pending")}
          </span>
        </button>
      ))}
    </div>
  )
}

function BatchPreview({ model }: { model: InboxModel }) {
  const preview = model.preview
  return (
    <section
      className="flex min-h-40 min-w-0 flex-col gap-2"
      aria-label={model.t("preview")}
    >
      {model.loading ? (
        <p
          role="status"
          className="flex items-center gap-2 text-sm text-muted-foreground"
        >
          <Loader2 className="size-4 animate-spin" />
          {model.t("preparing")}
        </p>
      ) : preview ? (
        <>
          <p className="text-xs text-muted-foreground tabular-nums">
            {preview.messageCount === null
              ? model.t("unknownCount")
              : model.t("messageCount", { count: preview.messageCount })}
            {" · "}
            {model.t("attachmentCount", { count: preview.attachmentCount })}
          </p>
          {preview.warning && (
            <p role="status" className="text-xs text-muted-foreground">
              {preview.warning}
            </p>
          )}
          <pre className="max-h-64 min-h-32 overflow-auto whitespace-pre-wrap break-words rounded-md border bg-muted/20 p-3 font-sans text-xs leading-relaxed md:max-h-72">
            {preview.preview || model.t("noText")}
          </pre>
          <p className="text-xs text-muted-foreground">
            {model.t("previewNote")}
          </p>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          {model.t("selectBatch")}
        </p>
      )}
    </section>
  )
}

function ConversationTarget({ model }: { model: InboxModel }) {
  const id = useId()
  return (
    <div className="grid gap-3 border-t pt-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
      <div className="grid min-w-0 gap-1.5">
        <label htmlFor={id} className="text-sm">
          {model.t("target")}
        </label>
        <select
          id={id}
          value={model.targetTab}
          disabled={model.busy}
          onChange={(event) => model.setTargetTab(event.target.value)}
          className="h-9 w-full rounded-md border bg-background px-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <option value="">{model.t("chooseConversation")}</option>
          {model.tabs.map((tab) => (
            <option key={tab.id} value={tab.id}>
              {tab.title || model.t("untitled")} ·{" "}
              {tab.workingDir || tab.agentType}
            </option>
          ))}
        </select>
        <p className="text-xs text-muted-foreground">{model.t("draftOnly")}</p>
      </div>
      <Button
        disabled={
          model.busy || model.loading || !model.preview || !model.targetTab
        }
        onClick={() => void model.addToDraft()}
      >
        {model.busy && <Loader2 className="size-4 animate-spin" />}
        {model.t("addToDraft")}
      </Button>
    </div>
  )
}
