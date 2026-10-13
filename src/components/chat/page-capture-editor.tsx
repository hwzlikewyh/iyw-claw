"use client"

import { useCallback, useState } from "react"
import { useTranslations } from "next-intl"
import {
  CornerUpLeft,
  Frame,
  MoveUpRight,
  RectangleHorizontal,
  Trash2,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { PageCaptureCanvas } from "./page-capture-canvas"
import {
  renderPageCapture,
  type PageCapture,
  type CaptureMark,
  type CaptureRegion,
} from "@/lib/page-capture"

type Props = {
  capture: PageCapture
  crop?: boolean
  onSend: (capture: PageCapture, note: string) => void
  onClose: () => void
}
const MAX_CAPTURE_NOTE_CHARS = 2000

export function PageCaptureEditor(props: Props) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const editor = useCaptureEditor(props)
  const { tool, marks, setMarks, region, setRegion, onError } = editor
  return (
    <section
      className="flex h-full min-h-0 flex-col gap-2 p-2"
      aria-label={t("markup")}
    >
      <CaptureTools editor={editor} />
      <p className="text-xs text-muted-foreground">{t("drawHint")}</p>
      <div className="min-h-0 flex-1 overflow-auto rounded-md border">
        <PageCaptureCanvas
          capture={props.capture}
          tool={tool}
          marks={marks}
          region={region}
          onMark={(mark) => setMarks([...marks, mark])}
          onRegion={setRegion}
          onError={onError}
          label={t("markup")}
        />
      </div>
      <CaptureSubmit editor={editor} onClose={props.onClose} />
    </section>
  )
}

function useCaptureEditor(props: Props) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const [tool, setTool] = useState<"crop" | "box" | "arrow">(
    props.crop ? "crop" : "box"
  )
  const history = useCaptureHistory()
  const [note, setNote] = useState("")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const onError = useCallback(() => setError(t("captureFailed")), [t])
  const send = async () => {
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      const capture = await renderPageCapture({
        capture: props.capture,
        marks: history.marks,
        region: history.region,
      })
      props.onSend(capture, note.trim())
    } catch (error) {
      setError(error instanceof Error ? error.message : t("captureFailed"))
    } finally {
      setBusy(false)
    }
  }
  return {
    tool,
    setTool,
    ...history,
    note,
    setNote,
    busy,
    error,
    onError,
    send,
  }
}

function useCaptureHistory() {
  const [history, setHistory] = useState<
    { marks: CaptureMark[]; region: CaptureRegion | null }[]
  >([{ marks: [], region: null }])
  const { marks, region } = history[history.length - 1]
  return {
    marks,
    region,
    setMarks: (marks: CaptureMark[]) =>
      setHistory([...history, { marks, region }]),
    setRegion: (region: CaptureRegion | null) =>
      setHistory([...history, { marks, region }]),
    canUndo: history.length > 1,
    undo: () => setHistory(history.slice(0, -1)),
    clear: () => setHistory([...history, { marks: [], region: null }]),
  }
}

type Editor = ReturnType<typeof useCaptureEditor>
function CaptureTools({ editor }: { editor: Editor }) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const { tool, setTool, canUndo, undo, clear } = editor
  return (
    <div className="flex flex-wrap gap-1">
      {[
        { id: "crop" as const, icon: Frame, label: t("region") },
        { id: "box" as const, icon: RectangleHorizontal, label: t("box") },
        { id: "arrow" as const, icon: MoveUpRight, label: t("arrow") },
      ].map(({ id, icon: Icon, label }) => (
        <Button
          key={id}
          size="icon-sm"
          variant={tool === id ? "secondary" : "ghost"}
          title={label}
          aria-label={label}
          aria-pressed={tool === id}
          onClick={() => setTool(id)}
        >
          <Icon />
        </Button>
      ))}
      <Button
        size="icon-sm"
        variant="ghost"
        title={t("undo")}
        aria-label={t("undo")}
        disabled={!canUndo}
        onClick={undo}
      >
        <CornerUpLeft />
      </Button>
      <Button
        size="icon-sm"
        variant="ghost"
        title={t("clear")}
        aria-label={t("clear")}
        onClick={clear}
      >
        <Trash2 />
      </Button>
    </div>
  )
}

function CaptureSubmit({
  editor,
  onClose,
}: {
  editor: Editor
  onClose: () => void
}) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const { error, note, setNote, busy, tool, region, send } = editor
  return (
    <>
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
      <textarea
        aria-label={t("note")}
        placeholder={t("note")}
        value={note}
        maxLength={MAX_CAPTURE_NOTE_CHARS}
        rows={2}
        onChange={(event) => setNote(event.target.value)}
        className="resize-none rounded-md border p-2 text-sm"
      />
      <div className="flex justify-end gap-2">
        <Button variant="ghost" size="sm" disabled={busy} onClick={onClose}>
          {t("cancel")}
        </Button>
        <Button
          size="sm"
          disabled={busy || (tool === "crop" && !region)}
          onClick={() => void send()}
        >
          {t("send")}
        </Button>
      </div>
    </>
  )
}
