"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { FileText, Brain, Loader2, RefreshCw, Save } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Textarea } from "@/components/ui/textarea"
import { UserMemoryPolicyPanel } from "./user-memory-policy-panel"
import { UserMemoryDiagnosticsPanel } from "./user-memory-diagnostics"
import { getUserMemorySettings, updateUserMemorySettings } from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"
import {
  buildUserMemoryUpdateRequest,
  createUserMemoryDraft,
  getUserMemoryDocument,
  userMemoryContainsEntryMarkers,
  userMemoryLineCount,
  USER_MEMORY_DOCUMENTS,
  type UserMemoryDocumentId,
  type UserMemoryDraft,
  type UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"

function sanitizeMemoryContent(text: string): string {
  return text
    .replace(/<!--.*?-->/gs, "")
    .replace(/\s*\[Codex CLI\]\s*/g, " ")
    .replace(/[ \t]+$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim()
}

export function UserMemorySettings() {
  const t = useTranslations("UserMemorySettings")
  const [activeDocumentId, setActiveDocumentId] =
    useState<UserMemoryDocumentId>("memory")
  const [settings, setSettings] = useState<UserMemorySettingsSnapshot | null>(
    null
  )
  const [draft, setDraft] = useState<UserMemoryDraft | null>(null)
  const [markerProtected, setMarkerProtected] = useState<
    Record<UserMemoryDocumentId, boolean>
  >({ memory: false, profile: false, soul: false })
  const [staleRunningSessions, setStaleRunningSessions] = useState(0)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeDocument = useMemo(
    () => getUserMemoryDocument(activeDocumentId),
    [activeDocumentId]
  )
  const activeSnapshot = settings?.documents[activeDocumentId] ?? null
  const content = draft?.documents[activeDocumentId].content ?? ""
  const updateRequest = useMemo(
    () =>
      settings && draft ? buildUserMemoryUpdateRequest(settings, draft) : null,
    [draft, settings]
  )
  const dirty = updateRequest !== null
  const stats = useMemo(
    () => ({
      chars: content.length,
      lines: userMemoryLineCount(content),
    }),
    [content]
  )

  const applySettings = useCallback((next: UserMemorySettingsSnapshot) => {
    // P0-4: marker-bearing documents must never be overwritten with
    // sanitized text. Track protection from the RAW persisted content before
    // sanitizing for display.
    setMarkerProtected({
      memory: userMemoryContainsEntryMarkers(next.documents.memory.content),
      profile: userMemoryContainsEntryMarkers(next.documents.profile.content),
      soul: userMemoryContainsEntryMarkers(next.documents.soul.content),
    })
    const cleaned: UserMemorySettingsSnapshot = {
      ...next,
      documents: {
        ...next.documents,
        memory: {
          ...next.documents.memory,
          content: sanitizeMemoryContent(next.documents.memory.content),
        },
      },
    }
    setSettings(cleaned)
    setDraft(createUserMemoryDraft(cleaned))
    setStaleRunningSessions(next.staleRunningSessions)
  }, [])

  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      applySettings(await getUserMemorySettings())
    } catch (err) {
      setError(toErrorMessage(err))
    } finally {
      setLoading(false)
    }
  }, [applySettings])

  useEffect(() => {
    load().catch((err) => {
      console.error("[UserMemorySettings] load failed:", err)
    })
  }, [load])

  const reload = useCallback(() => {
    if (dirty && !window.confirm(t("discardChangesConfirm"))) return
    void load()
  }, [dirty, load, t])

  const save = useCallback(async () => {
    if (!updateRequest) return
    // P0-4: refuse to overwrite a marker-bearing document with whole-document
    // content (defense in depth; the editor is also locked for these docs).
    const blocked = Object.entries(updateRequest.documents ?? {}).some(
      ([id, patch]) =>
        patch.content !== undefined &&
        markerProtected[id as UserMemoryDocumentId]
    )
    if (blocked) {
      setError(t("markerProtectedSaveBlocked"))
      toast.error(t("markerProtectedSaveBlocked"))
      return
    }
    setSaving(true)
    setError(null)
    try {
      const result = await updateUserMemorySettings(updateRequest)
      applySettings(result.settings)
      setStaleRunningSessions(result.affectedRunningSessions)
      toast.success(t("saved"))
    } catch (err) {
      const conflict = extractAppCommandError(err)?.code === "conflict"
      const message = conflict ? t("saveConflict") : toErrorMessage(err)
      setError(message)
      toast.error(conflict ? t("saveConflict") : t("saveFailed"), {
        description: conflict ? toErrorMessage(err) : message,
      })
    } finally {
      setSaving(false)
    }
  }, [applySettings, markerProtected, t, updateRequest])

  const switchDocument = useCallback((nextId: UserMemoryDocumentId) => {
    setActiveDocumentId(nextId)
  }, [])

  if (loading) {
    return (
      <div
        aria-busy="true"
        className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground"
      >
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  if (!settings || !draft) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-4 text-sm">
        {error && (
          <div
            role="alert"
            className="max-w-lg rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-center text-xs text-red-400"
          >
            {error}
          </div>
        )}
        <Button
          size="sm"
          variant="outline"
          onClick={() => void load()}
          disabled={loading}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t("reload")}
        </Button>
      </div>
    )
  }

  const readonly = activeSnapshot?.readonly ?? false
  const fullPath = activeSnapshot?.path ?? activeDocument.fileName
  const markerLocked = markerProtected[activeDocumentId] ?? false
  // P0-4: never show "active" when the backend is actually unavailable —
  // surface the availability/companion/candidate diagnostics instead.
  const availabilityDown =
    !!settings.availability && !settings.availability.available
  const companionDown =
    !!settings.companionHealth && settings.companionHealth.status !== "ready"
  const candidateDown =
    !!settings.candidateDiagnostic && !settings.candidateDiagnostic.available
  const backendHealthy = !availabilityDown && !companionDown && !candidateDown

  const diagnosticDetail = availabilityDown
    ? settings.availability?.detail
    : companionDown
      ? settings.companionHealth?.detail
      : candidateDown
        ? settings.candidateDiagnostic?.detail
        : null

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Brain}
        title={t("title")}
        description={t("description")}
        action={
          <div className="flex flex-wrap items-center gap-2">
            {availabilityDown && (
              <Badge variant="destructive" className="text-[11px]">
                {t("status.unavailable")}
              </Badge>
            )}
            {!availabilityDown && companionDown && (
              <Badge variant="destructive" className="text-[11px]">
                {t("status.companionUnavailable")}
              </Badge>
            )}
            {!availabilityDown && !companionDown && candidateDown && (
              <Badge variant="destructive" className="text-[11px]">
                {t("status.candidateUnavailable")}
              </Badge>
            )}
            {backendHealthy && !settings.enabled && (
              <Badge variant="outline" className="text-[11px]">
                {t("status.disabled")}
              </Badge>
            )}
            {backendHealthy && staleRunningSessions > 0 ? (
              <Badge variant="destructive" className="text-[11px]">
                {t("status.newConversationRequired", {
                  count: staleRunningSessions,
                })}
              </Badge>
            ) : (
              backendHealthy &&
              settings.enabled && (
                <Badge variant="outline" className="text-[11px]">
                  {t("status.active")}
                </Badge>
              )
            )}
            <Button
              size="sm"
              variant="outline"
              onClick={reload}
              disabled={saving}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("reload")}
            </Button>
            <Button
              size="sm"
              onClick={() => void save()}
              disabled={!dirty || saving}
            >
              {saving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Save className="h-3.5 w-3.5" />
              )}
              {saving ? t("saving") : t("save")}
            </Button>
          </div>
        }
      />

      {draft && (
        <UserMemoryPolicyPanel
          draft={draft}
          disabled={saving}
          onChange={setDraft}
        />
      )}

      <UserMemoryDiagnosticsPanel settings={settings} busy={saving} />

      {!backendHealthy && (
        <div
          role="alert"
          className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400"
        >
          {diagnosticDetail ?? t("status.unavailable")}
        </div>
      )}

      <section className="grid gap-2 sm:grid-cols-3">
        {USER_MEMORY_DOCUMENTS.map((document) => {
          const active = document.id === activeDocumentId
          return (
            <button
              key={document.id}
              type="button"
              onClick={() => switchDocument(document.id)}
              aria-current={active ? "page" : undefined}
              className={`rounded-xl border p-3 text-left transition-colors ${
                active
                  ? "border-primary/60 bg-primary/10"
                  : "bg-card hover:bg-muted/40"
              }`}
            >
              <div className="text-sm font-medium">{t(document.labelKey)}</div>
              <div className="mt-1 text-xs leading-5 text-muted-foreground">
                {t(document.descriptionKey)}
              </div>
            </button>
          )
        })}
      </section>

      {error && (
        <div
          role="alert"
          className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400"
        >
          {error}
        </div>
      )}

      <section className="flex flex-col gap-3 rounded-xl border bg-card p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2 text-xs text-muted-foreground">
            <FileText className="h-4 w-4 shrink-0" />
            <span className="truncate font-mono">
              {t(activeDocument.labelKey)} · {fullPath}
            </span>
          </div>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            {dirty && <span className="text-amber-500">{t("dirty")}</span>}
            {readonly && <span className="text-red-400">{t("readonly")}</span>}
            {markerLocked && (
              <span className="text-amber-500">{t("markerProtected")}</span>
            )}
            <span>{t("stats", stats)}</span>
          </div>
        </div>

        {markerLocked && (
          <p className="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2 text-xs text-amber-500">
            {t("markerProtectedHint")}
          </p>
        )}

        <Textarea
          value={content}
          onChange={(event) => {
            const nextContent = event.target.value
            setDraft((current) =>
              current
                ? {
                    ...current,
                    documents: {
                      ...current.documents,
                      [activeDocumentId]: {
                        ...current.documents[activeDocumentId],
                        content: nextContent,
                      },
                    },
                  }
                : current
            )
          }}
          placeholder={t(activeDocument.placeholderKey)}
          disabled={readonly || markerLocked || saving}
          className="min-h-[420px] flex-1 resize-none font-mono text-sm leading-6"
        />
      </section>
    </SettingsPageLayout>
  )
}
