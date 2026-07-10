"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { FileText, Loader2, RefreshCw, Save } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Textarea } from "@/components/ui/textarea"
import {
  createFileTreeEntry,
  getHomeDirectory,
  readFileForEdit,
  saveFileContent,
} from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import type { FileEditContent } from "@/lib/types"
import {
  displayUserMemoryPath,
  getUserMemoryDocument,
  userMemoryLineCount,
  userMemoryRelativePath,
  USER_MEMORY_DIR,
  USER_MEMORY_DOCUMENTS,
  type UserMemoryDocument,
  type UserMemoryDocumentId,
} from "@/lib/user-memory-documents"

function isErrorCode(error: unknown, code: string): boolean {
  return extractAppCommandError(error)?.code === code
}

async function ignoreAlreadyExists(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (err) {
    if (!isErrorCode(err, "already_exists")) throw err
  }
}

async function ensureMemoryFile(
  homePath: string,
  document: UserMemoryDocument
): Promise<FileEditContent> {
  const relativePath = userMemoryRelativePath(document)
  try {
    return await readFileForEdit(homePath, relativePath)
  } catch (err) {
    if (!isErrorCode(err, "not_found")) throw err
  }

  await ignoreAlreadyExists(() =>
    createFileTreeEntry(homePath, "", USER_MEMORY_DIR, "dir")
  )
  await ignoreAlreadyExists(() =>
    createFileTreeEntry(homePath, USER_MEMORY_DIR, document.fileName, "file")
  )
  return readFileForEdit(homePath, relativePath)
}

export function UserMemorySettings() {
  const t = useTranslations("UserMemorySettings")
  const [activeDocumentId, setActiveDocumentId] =
    useState<UserMemoryDocumentId>("memory")
  const [homePath, setHomePath] = useState<string | null>(null)
  const [content, setContent] = useState("")
  const [savedContent, setSavedContent] = useState("")
  const [etag, setEtag] = useState<string | null>(null)
  const [readonly, setReadonly] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeDocument = useMemo(
    () => getUserMemoryDocument(activeDocumentId),
    [activeDocumentId]
  )
  const relativePath = userMemoryRelativePath(activeDocument)
  const fullPath = displayUserMemoryPath(homePath, relativePath)
  const dirty = content !== savedContent
  const stats = useMemo(
    () => ({
      chars: content.length,
      lines: userMemoryLineCount(content),
    }),
    [content]
  )

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const home = await getHomeDirectory()
      setHomePath(home)
      const file = await ensureMemoryFile(home, activeDocument)
      setContent(file.content)
      setSavedContent(file.content)
      setEtag(file.etag)
      setReadonly(file.readonly)
    } catch (err) {
      setError(toErrorMessage(err))
    } finally {
      setLoading(false)
    }
  }, [activeDocument])

  useEffect(() => {
    load().catch((err) => {
      console.error("[UserMemorySettings] load failed:", err)
    })
  }, [load])

  const save = useCallback(async () => {
    if (!homePath || etag === null || readonly) return
    setSaving(true)
    setError(null)
    try {
      const result = await saveFileContent(
        homePath,
        relativePath,
        content,
        etag
      )
      setSavedContent(content)
      setEtag(result.etag)
      setReadonly(result.readonly)
      toast.success(t("saved"))
    } catch (err) {
      const message = toErrorMessage(err)
      setError(message)
      toast.error(t("saveFailed"), { description: message })
    } finally {
      setSaving(false)
    }
  }, [content, etag, homePath, readonly, relativePath, t])

  const switchDocument = useCallback(
    (nextId: UserMemoryDocumentId) => {
      if (nextId === activeDocumentId) return
      if (dirty && !window.confirm(t("discardChangesConfirm"))) return
      setActiveDocumentId(nextId)
    },
    [activeDocumentId, dirty, t]
  )

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <ScrollArea className="h-full">
      <div className="flex min-h-full flex-col gap-4 p-3 md:p-4">
        <section className="flex flex-wrap items-start justify-between gap-3">
          <div className="space-y-1">
            <h1 className="text-sm font-semibold">{t("title")}</h1>
            <p className="text-xs text-muted-foreground">{t("description")}</p>
          </div>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                setLoading(true)
                load().catch((err) => {
                  console.error("[UserMemorySettings] reload failed:", err)
                })
              }}
              disabled={saving}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("reload")}
            </Button>
            <Button
              size="sm"
              onClick={() => {
                save().catch((err) => {
                  console.error("[UserMemorySettings] save failed:", err)
                })
              }}
              disabled={!dirty || saving || readonly}
            >
              {saving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Save className="h-3.5 w-3.5" />
              )}
              {saving ? t("saving") : t("save")}
            </Button>
          </div>
        </section>

        <section className="grid gap-2 sm:grid-cols-3">
          {USER_MEMORY_DOCUMENTS.map((document) => {
            const active = document.id === activeDocumentId
            return (
              <button
                key={document.id}
                type="button"
                onClick={() => switchDocument(document.id)}
                aria-current={active ? "page" : undefined}
                className={`rounded-lg border p-3 text-left transition-colors ${
                  active
                    ? "border-primary/60 bg-primary/10"
                    : "bg-card hover:bg-muted/40"
                }`}
              >
                <div className="text-sm font-medium">
                  {t(document.labelKey)}
                </div>
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {t(document.descriptionKey)}
                </div>
              </button>
            )
          })}
        </section>

        {error && (
          <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
            {error}
          </div>
        )}

        <section className="flex flex-1 flex-col gap-3 rounded-lg border bg-card p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
              <FileText className="h-4 w-4 shrink-0" />
              <span className="truncate font-mono">
                {t(activeDocument.labelKey)} · {fullPath}
              </span>
            </div>
            <div className="flex items-center gap-3 text-xs text-muted-foreground">
              {dirty && <span className="text-amber-500">{t("dirty")}</span>}
              {readonly && (
                <span className="text-red-400">{t("readonly")}</span>
              )}
              <span>{t("stats", stats)}</span>
            </div>
          </div>

          <Textarea
            value={content}
            onChange={(event) => setContent(event.target.value)}
            placeholder={t(activeDocument.placeholderKey)}
            disabled={readonly || saving}
            className="min-h-[420px] flex-1 resize-none font-mono text-sm leading-6"
          />
        </section>
      </div>
    </ScrollArea>
  )
}
