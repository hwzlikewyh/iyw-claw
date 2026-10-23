"use client"

import { useCallback, useEffect, useState } from "react"
import { Loader2, Pencil, Plus, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import {
  createLibraryPoint,
  deleteLibraryPoint,
  getLibraryPoints,
  normalizeLibraryPoint,
  updateLibraryPoint,
  type LibraryFile,
  type LibraryPoint,
  type LibraryTarget,
} from "@/lib/knowledge-library"
import { knowledgeError } from "@/lib/cloud-knowledge"

export function LibraryPointsDialog({
  file,
  target,
  transport,
  onClose,
}: {
  file: LibraryFile
  target: LibraryTarget
  transport: Parameters<typeof getLibraryPoints>[0]
  onClose: () => void
}) {
  const t = useTranslations("Library")
  const [points, setPoints] = useState<LibraryPoint[]>([])
  const [editing, setEditing] = useState<LibraryPoint | null>(null)
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const load = useCallback(async () => {
    setLoading(true)
    setError("")
    try {
      const result = await getLibraryPoints(transport, {
        scope: target.scope,
        category: target.category,
        fileId: file.fileId,
      })
      setPoints((result.list ?? []).map(normalizeLibraryPoint))
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setLoading(false)
    }
  }, [file.fileId, target.category, target.scope, transport])
  useEffect(() => {
    void load()
  }, [load])
  const save = async (form: PointForm) => {
    setBusy(true)
    setError("")
    try {
      const input = {
        scope: target.scope,
        category: target.category,
        fileId: file.fileId,
        chunkTitle: form.title.trim(),
        content: form.content.trim(),
        question: form.question.trim(),
      }
      if (editing)
        await updateLibraryPoint(transport, {
          ...input,
          pointId: editing.pointId,
        })
      else await createLibraryPoint(transport, input)
      setEditing(null)
      await load()
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setBusy(false)
    }
  }
  const remove = async (point: LibraryPoint) => {
    setBusy(true)
    setError("")
    try {
      await deleteLibraryPoint(transport, {
        scope: target.scope,
        category: target.category,
        fileId: file.fileId,
        pointId: point.pointId,
      })
      await load()
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="flex h-[min(46rem,calc(100dvh-2rem))] max-w-3xl flex-col gap-3 rounded-lg p-4 sm:p-6">
        <DialogHeader>
          <DialogTitle>{t("sliceManagement")}</DialogTitle>
          <DialogDescription className="truncate">
            {file.fileName}
          </DialogDescription>
        </DialogHeader>
        {editing ? (
          <PointForm
            initial={{
              title: editing.chunkTitle,
              content: editing.content,
              question: editing.question,
            }}
            busy={busy}
            onCancel={() => setEditing(null)}
            onSave={save}
          />
        ) : (
          <>
            <div className="flex items-center justify-between">
              <p className="text-xs text-muted-foreground">
                {t("sliceCount", { count: points.length })}
              </p>
              <Button
                size="sm"
                onClick={() =>
                  setEditing({
                    pointId: "",
                    chunkTitle: "",
                    content: "",
                    question: "",
                    chunkType: "text",
                    raw: {},
                  })
                }
              >
                <Plus className="size-3.5" />
                {t("addSlice")}
              </Button>
            </div>
            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto pr-1">
              {loading ? (
                <div className="flex justify-center p-8">
                  <Loader2 className="size-5 animate-spin" />
                </div>
              ) : points.length ? (
                points.map((point) => (
                  <PointCard
                    key={point.pointId}
                    point={point}
                    busy={busy}
                    onEdit={() => setEditing(point)}
                    onDelete={() => void remove(point)}
                  />
                ))
              ) : (
                <p className="p-8 text-center text-sm text-muted-foreground">
                  {t("noSlices")}
                </p>
              )}
            </div>
            {error && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}

interface PointForm {
  title: string
  content: string
  question: string
}
function PointForm({
  initial,
  busy,
  onCancel,
  onSave,
}: {
  initial: PointForm
  busy: boolean
  onCancel: () => void
  onSave: (form: PointForm) => Promise<void>
}) {
  const t = useTranslations("Library")
  const [form, setForm] = useState(initial)
  return (
    <form
      className="flex min-h-0 flex-1 flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault()
        void onSave(form)
      }}
    >
      <label className="space-y-1 text-sm">
        <span>{t("sliceTitle")}</span>
        <Input
          value={form.title}
          onChange={(e) => setForm({ ...form, title: e.target.value })}
          maxLength={200}
        />
      </label>
      <label className="space-y-1 text-sm">
        <span>{t("sliceContent")}</span>
        <Textarea
          className="min-h-40 flex-1"
          value={form.content}
          onChange={(e) => setForm({ ...form, content: e.target.value })}
          maxLength={5000}
          required
        />
      </label>
      <label className="space-y-1 text-sm">
        <span>{t("sliceQuestion")}</span>
        <Input
          value={form.question}
          onChange={(e) => setForm({ ...form, question: e.target.value })}
          maxLength={1000}
        />
      </label>
      <DialogFooter>
        <Button
          type="button"
          variant="outline"
          onClick={onCancel}
          disabled={busy}
        >
          {t("cancel")}
        </Button>
        <Button type="submit" disabled={busy || !form.content.trim()}>
          {busy && <Loader2 className="size-4 animate-spin" />}
          {t("save")}
        </Button>
      </DialogFooter>
    </form>
  )
}

function PointCard({
  point,
  busy,
  onEdit,
  onDelete,
}: {
  point: LibraryPoint
  busy: boolean
  onEdit: () => void
  onDelete: () => void
}) {
  const t = useTranslations("Library")
  return (
    <article className="rounded-md border p-3">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h3 className="truncate text-sm font-medium">
            {point.chunkTitle || t("untitledSlice")}
          </h3>
          <p className="mt-1 text-xs text-muted-foreground">{point.pointId}</p>
        </div>
        <div className="flex shrink-0 gap-1">
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onEdit}
            disabled={busy}
            aria-label={t("editSlice")}
          >
            <Pencil className="size-3.5" />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            onClick={onDelete}
            disabled={busy}
            aria-label={t("deleteSlice")}
          >
            <Trash2 className="size-3.5" />
          </Button>
        </div>
      </div>
      <p className="mt-3 line-clamp-5 whitespace-pre-wrap text-sm text-muted-foreground">
        {point.content}
      </p>
      {point.question && (
        <p className="mt-2 text-xs text-muted-foreground">
          {t("sliceQuestion")}: {point.question}
        </p>
      )}
    </article>
  )
}
