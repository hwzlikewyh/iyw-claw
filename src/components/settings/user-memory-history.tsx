"use client"

import { useEffect, useState } from "react"
import { Check, Clock3, ExternalLink, History, Loader2, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemoryHistory,
  getMemoryReceipts,
  recordMemoryRecallFeedback,
  type MemoryRevisionEntry,
  type MemoryRecallReceipt,
} from "@/lib/user-memory-authority"

export function MemoryHistoryButton({ id }: { id: string }) {
  const t = useTranslations("UserMemorySettings.authority")
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        variant="ghost"
        size="icon"
        title={t("history")}
        aria-label={t("history")}
        onClick={() => setOpen(true)}
      >
        <History className="size-4" />
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{t("history")}</DialogTitle>
            <DialogDescription className="break-all font-mono text-xs">
              {id}
            </DialogDescription>
          </DialogHeader>
          {open && <HistoryContent id={id} onClose={() => setOpen(false)} />}
        </DialogContent>
      </Dialog>
    </>
  )
}

function HistoryContent({ id, onClose }: { id: string; onClose: () => void }) {
  const t = useTranslations("UserMemorySettings.authority")
  const [data, setData] = useState<{
    history: MemoryRevisionEntry[]
    receipts: MemoryRecallReceipt[]
  } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [generation, setGeneration] = useState(0)
  useEffect(() => {
    let current = true
    Promise.all([getMemoryHistory(id), getMemoryReceipts(id)])
      .then(([history, receipts]) => current && setData({ history, receipts }))
      .catch((reason) => current && setError(toErrorMessage(reason)))
    return () => {
      current = false
    }
  }, [id, generation])
  if (error)
    return (
      <p role="alert" className="break-words text-sm text-destructive">
        {error}
      </p>
    )
  if (!data)
    return (
      <p role="status" className="flex gap-2 text-sm">
        <Loader2 className="size-4 animate-spin" />
        {t("loading")}
      </p>
    )
  return (
    <HistoryResult
      data={data}
      onClose={onClose}
      onFeedback={() => setGeneration((value) => value + 1)}
    />
  )
}

function HistoryResult({
  data,
  onClose,
  onFeedback,
}: {
  data: { history: MemoryRevisionEntry[]; receipts: MemoryRecallReceipt[] }
  onClose: () => void
  onFeedback: () => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  return (
    <div className="min-w-0 space-y-4">
      {data.history.length === 0 && (
        <p className="text-sm text-muted-foreground">{t("noHistory")}</p>
      )}
      <ol className="divide-y">
        {data.history.map((revision) => (
          <RevisionRow
            key={revision.revision}
            entry={revision}
            onClose={onClose}
          />
        ))}
      </ol>
      <h3 className="text-sm font-medium">{t("receipts")}</h3>
      {data.receipts.length === 0 ? (
        <p className="text-xs text-muted-foreground">{t("noReceipts")}</p>
      ) : (
        <ul className="space-y-2 text-xs text-muted-foreground">
          {data.receipts.map((receipt) => (
            <li key={receipt.id} className="break-words">
              {new Date(receipt.recordedAt).toLocaleString()} · {t("delivered")}{" "}
              · v{receipt.items.map((item) => item.revision).join(", ")}
              {receipt.turnNonce !== null &&
                ` · ${t("turn")} ${receipt.turnNonce}`}
              <ReceiptFeedback receipt={receipt} onUpdated={onFeedback} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function ReceiptFeedback({
  receipt,
  onUpdated,
}: {
  receipt: MemoryRecallReceipt
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  const [busy, setBusy] = useState(false)
  const recordId = receipt.items[0]?.recordId
  const apply = async (verdict: "used" | "irrelevant" | "outdated") => {
    if (!recordId || busy) return
    setBusy(true)
    try {
      await recordMemoryRecallFeedback({
        receiptId: receipt.id,
        recordId,
        verdict,
      })
      onUpdated()
    } catch (reason) {
      toast.error(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <span className="mt-1 flex gap-1">
      {(["used", "irrelevant", "outdated"] as const).map((verdict) => (
        <Button
          key={verdict}
          size="icon"
          variant={receipt.feedback === verdict ? "secondary" : "ghost"}
          title={t(verdict)}
          aria-label={t(verdict)}
          disabled={busy || !recordId}
          onClick={() => void apply(verdict)}
        >
          {verdict === "used" ? (
            <Check className="size-3" />
          ) : verdict === "irrelevant" ? (
            <X className="size-3" />
          ) : (
            <Clock3 className="size-3" />
          )}
        </Button>
      ))}
    </span>
  )
}

function RevisionRow({
  entry,
  onClose,
}: {
  entry: MemoryRevisionEntry
  onClose: () => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  return (
    <li className="space-y-2 py-3">
      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
        <span>v{entry.revision}</span>
        <span>{new Date(entry.recordedAt).toLocaleString()}</span>
        <span>
          {t.has(entry.state as "active")
            ? t(entry.state as "active")
            : entry.state}
        </span>
      </div>
      <p className="whitespace-pre-wrap break-words text-sm">
        {entry.item.redacted ? t("redacted") : entry.item.item?.content}
      </p>
      {entry.item.item?.valid_to && (
        <p className="text-xs text-muted-foreground">
          {t("validTo")}: {new Date(entry.item.item.valid_to).toLocaleString()}
        </p>
      )}
      {entry.item.relations?.map((relation) => (
        <p
          key={`${relation.relation}:${relation.target_id}`}
          className="break-all font-mono text-xs text-muted-foreground"
        >
          {relation.relation}: {relation.target_id}
        </p>
      ))}
      <HistorySources sources={entry.sources} onClose={onClose} />
    </li>
  )
}

function HistorySources({
  sources,
  onClose,
}: {
  sources: MemoryRevisionEntry["sources"]
  onClose: () => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  return (
    <ul className="space-y-1">
      {sources.map((source, index) => (
        <li
          key={`${source.sourceId}:${index}`}
          className="break-all text-xs text-muted-foreground"
        >
          {source.conversation ? (
            <Button
              variant="link"
              className="h-auto max-w-full justify-start whitespace-normal p-0 text-xs"
              onClick={() => openSourceConversation(source, onClose)}
            >
              <ExternalLink className="size-3 shrink-0" />
              {source.conversation.title || t("sourceConversation")} ·{" "}
              {t("turn")} {source.turnNonce}
            </Button>
          ) : (
            <span className="font-mono">
              {source.sourceId}
              {source.availability === "deleted_or_missing" && (
                <span className="ml-2 font-sans">{t("sourceUnavailable")}</span>
              )}
            </span>
          )}
        </li>
      ))}
    </ul>
  )
}

function openSourceConversation(
  source: MemoryRevisionEntry["sources"][number],
  onClose: () => void
) {
  if (!source.conversation) return
  const conversation = source.conversation
  const params = new URLSearchParams(window.location.search)
  params.set("folderId", String(conversation.folderId))
  params.set("conversationId", String(conversation.id))
  params.set("agent", conversation.agentType)
  window.location.assign(`/workspace?${params}`)
  onClose()
}
