"use client"

import { useState } from "react"
import { Code, FileText, List } from "lucide-react"
import { useTranslations } from "next-intl"

import { Textarea } from "@/components/ui/textarea"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  getUserMemoryDocument,
  userMemoryLineCount,
  USER_MEMORY_DOCUMENTS,
  type UserMemoryDocumentId,
  type UserMemoryDraft,
  type UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"
import { UserMemoryEntriesPanel } from "./user-memory-entries-panel"
import { UserMemoryClearDialog } from "./user-memory-clear-dialog"

interface UserMemoryDocumentEditorProps {
  activeDocumentId: UserMemoryDocumentId
  settings: UserMemorySettingsSnapshot
  draft: UserMemoryDraft
  markerProtected: Record<UserMemoryDocumentId, boolean>
  dirty: boolean
  saving: boolean
  onDocumentChange: (documentId: UserMemoryDocumentId) => void
  onDraftChange: (draft: UserMemoryDraft) => void
  onEntryUpdated: () => void
}

type EditorView = UserMemoryDocumentEditorProps & {
  document: ReturnType<typeof getUserMemoryDocument>
  content: string
  markerLocked: boolean
  readonly: boolean
}

function DocumentEditorHeader({
  activeDocumentId,
  onDocumentChange,
}: Pick<EditorView, "activeDocumentId" | "onDocumentChange">) {
  const t = useTranslations("UserMemorySettings")
  return (
    <div className="border-b px-4 py-3">
      <div className="flex items-start gap-2">
        <FileText
          className="mt-0.5 h-4 w-4 text-muted-foreground"
          aria-hidden
        />
        <div>
          <h2 className="text-sm font-semibold">{t("manual.title")}</h2>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            {t("manual.description")}
          </p>
        </div>
      </div>
      <Tabs
        value={activeDocumentId}
        onValueChange={(value) =>
          onDocumentChange(value as UserMemoryDocumentId)
        }
        className="mt-3"
      >
        <TabsList className="grid w-full grid-cols-3">
          {USER_MEMORY_DOCUMENTS.map((item) => (
            <TabsTrigger key={item.id} value={item.id}>
              {t(item.labelKey)}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </div>
  )
}

function DocumentEditorMeta({
  settings,
  activeDocumentId,
  document,
  content,
  dirty,
  readonly,
  markerLocked,
}: Pick<
  EditorView,
  | "settings"
  | "activeDocumentId"
  | "document"
  | "content"
  | "dirty"
  | "readonly"
  | "markerLocked"
>) {
  const t = useTranslations("UserMemorySettings")
  const snapshot = settings.documents[activeDocumentId]
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
      <span className="min-w-0 truncate font-mono">
        {snapshot.path ?? document.fileName}
      </span>
      <span className="flex flex-wrap items-center gap-3">
        {dirty && <span className="text-amber-500">{t("dirty")}</span>}
        {readonly && <span className="text-red-400">{t("readonly")}</span>}
        {markerLocked && (
          <span className="text-amber-500">{t("markerProtected")}</span>
        )}
        <span>
          {t("stats", {
            chars: content.length,
            lines: userMemoryLineCount(content),
          })}
        </span>
      </span>
    </div>
  )
}

function DocumentEditorBody(view: EditorView) {
  const t = useTranslations("UserMemorySettings")
  const updateContent = (content: string) =>
    view.onDraftChange({
      ...view.draft,
      documents: {
        ...view.draft.documents,
        [view.activeDocumentId]: {
          ...view.draft.documents[view.activeDocumentId],
          content,
        },
      },
    })
  return (
    <div className="space-y-3 p-4">
      <DocumentEditorMeta {...view} />
      {view.markerLocked && (
        <p className="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-500">
          {t("markerProtectedHint")}
        </p>
      )}
      <Textarea
        value={view.content}
        onChange={(event) => updateContent(event.target.value)}
        placeholder={t(view.document.placeholderKey)}
        disabled={view.readonly || view.markerLocked || view.saving}
        className="min-h-72 resize-y font-mono text-sm leading-6"
      />
    </div>
  )
}

export function UserMemoryDocumentEditor(props: UserMemoryDocumentEditorProps) {
  const [view, setView] = useState<"entries" | "source">("entries")
  const editor: EditorView = {
    ...props,
    document: getUserMemoryDocument(props.activeDocumentId),
    content: props.draft.documents[props.activeDocumentId].content,
    markerLocked: props.markerProtected[props.activeDocumentId] ?? false,
    readonly:
      props.settings.documents[props.activeDocumentId].readonly ?? false,
  }
  return (
    <section className="overflow-hidden rounded-md border bg-card">
      <DocumentEditorHeader
        activeDocumentId={props.activeDocumentId}
        onDocumentChange={props.onDocumentChange}
      />
      <EditorViewButtons
        view={view}
        onChange={setView}
        saving={props.saving}
        dirty={props.dirty}
        onUpdated={props.onEntryUpdated}
      />
      {view === "entries" ? (
        <UserMemoryEntriesPanel
          key={`${props.activeDocumentId}:${props.settings.revision}`}
          document={props.activeDocumentId}
          disabled={props.saving || props.dirty}
          onUpdated={props.onEntryUpdated}
        />
      ) : (
        <DocumentEditorBody {...editor} />
      )}
    </section>
  )
}

function EditorViewButtons({
  view,
  onChange,
  saving,
  dirty,
  onUpdated,
}: {
  view: "entries" | "source"
  onChange: (value: "entries" | "source") => void
  saving: boolean
  dirty: boolean
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <div className="flex flex-wrap items-center gap-1 border-b px-4 py-1">
      <div className="mr-auto">
        <UserMemoryClearDialog
          disabled={saving || dirty}
          onUpdated={onUpdated}
        />
      </div>
      <Button
        size="icon"
        variant={view === "entries" ? "secondary" : "ghost"}
        title={t("listView")}
        aria-label={t("listView")}
        aria-pressed={view === "entries"}
        disabled={saving || dirty}
        onClick={() => onChange("entries")}
      >
        <List className="size-4" />
      </Button>
      <Button
        size="icon"
        variant={view === "source" ? "secondary" : "ghost"}
        title={t("sourceView")}
        aria-label={t("sourceView")}
        aria-pressed={view === "source"}
        disabled={saving}
        onClick={() => onChange("source")}
      >
        <Code className="size-4" />
      </Button>
    </div>
  )
}
