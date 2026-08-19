"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { getUserMemorySettings, updateUserMemorySettings } from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  buildUserMemoryUpdateRequest,
  createUserMemoryDraft,
  userMemoryContainsEntryMarkers,
  type UserMemoryDocumentId,
  type UserMemoryDraft,
  type UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"

type MarkerProtection = Record<UserMemoryDocumentId, boolean>

function memoryEntryMarkers(text: string): string[] {
  return Array.from(
    text.matchAll(/<!--\s*(iyw-memory-(?:fallback-)?[0-9a-f]+)\s*-->/g),
    (match) => match[1]
  )
}

function preservesMemoryEntryMarkers(saved: string, next: string): boolean {
  const savedMarkers = memoryEntryMarkers(saved)
  const nextMarkers = memoryEntryMarkers(next)
  return (
    savedMarkers.length === nextMarkers.length &&
    savedMarkers.every((marker, index) => marker === nextMarkers[index])
  )
}

function displaySnapshot(next: UserMemorySettingsSnapshot) {
  const markerProtected: MarkerProtection = {
    memory: userMemoryContainsEntryMarkers(next.documents.memory.content),
    profile: userMemoryContainsEntryMarkers(next.documents.profile.content),
    soul: userMemoryContainsEntryMarkers(next.documents.soul.content),
  }
  return {
    markerProtected,
    settings: next,
    draft: createUserMemoryDraft(next),
  }
}

function useMemorySnapshot() {
  const [settings, setSettings] = useState<UserMemorySettingsSnapshot | null>(
    null
  )
  const [draft, setDraft] = useState<UserMemoryDraft | null>(null)
  const [markerProtected, setMarkerProtected] = useState<MarkerProtection>({
    memory: false,
    profile: false,
    soul: false,
  })
  const [staleRunningSessions, setStaleRunningSessions] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const applySettings = useCallback((next: UserMemorySettingsSnapshot) => {
    const view = displaySnapshot(next)
    setMarkerProtected(view.markerProtected)
    setSettings(view.settings)
    setDraft(view.draft)
    setStaleRunningSessions(next.staleRunningSessions)
  }, [])
  const load = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      applySettings(await getUserMemorySettings())
    } catch (loadError) {
      setError(toErrorMessage(loadError))
    } finally {
      setLoading(false)
    }
  }, [applySettings])

  useEffect(() => void load(), [load])
  return {
    settings,
    draft,
    markerProtected,
    staleRunningSessions,
    loading,
    error,
    setDraft,
    setError,
    setStaleRunningSessions,
    applySettings,
    load,
  }
}

function useMemorySave(snapshot: ReturnType<typeof useMemorySnapshot>) {
  const t = useTranslations("UserMemorySettings")
  const [saving, setSaving] = useState(false)
  const updateRequest = useMemo(
    () =>
      snapshot.settings && snapshot.draft
        ? buildUserMemoryUpdateRequest(snapshot.settings, snapshot.draft)
        : null,
    [snapshot.draft, snapshot.settings]
  )
  const save = useCallback(async () => {
    if (!updateRequest) return
    const blocked = Object.entries(updateRequest.documents ?? {}).some(
      ([id, patch]) => {
        if (patch.content === undefined) return false
        const documentId = id as UserMemoryDocumentId
        return !preservesMemoryEntryMarkers(
          snapshot.settings?.documents[documentId].content ?? "",
          patch.content
        )
      }
    )
    if (blocked) {
      return reportBlockedSave(
        snapshot.setError,
        t("markerProtectedSaveBlocked")
      )
    }
    setSaving(true)
    snapshot.setError(null)
    try {
      const result = await updateUserMemorySettings(updateRequest)
      snapshot.applySettings(result.settings)
      snapshot.setStaleRunningSessions(result.affectedRunningSessions)
      toast.success(t("saved"))
    } catch (saveError) {
      reportSaveError(saveError, snapshot.setError, {
        conflict: t("saveConflict"),
        saveFailed: t("saveFailed"),
      })
    } finally {
      setSaving(false)
    }
  }, [snapshot, t, updateRequest])
  return { dirty: updateRequest !== null, saving, save }
}

function reportBlockedSave(setError: (value: string) => void, message: string) {
  setError(message)
  toast.error(message)
}

function reportSaveError(
  error: unknown,
  setError: (value: string) => void,
  messages: { conflict: string; saveFailed: string }
) {
  const conflict = extractAppCommandError(error)?.code === "conflict"
  const message = conflict ? messages.conflict : toErrorMessage(error)
  setError(message)
  toast.error(conflict ? messages.conflict : messages.saveFailed, {
    description: conflict ? toErrorMessage(error) : message,
  })
}

export function useUserMemorySettingsState() {
  const t = useTranslations("UserMemorySettings")
  const [activeDocumentId, setActiveDocumentId] =
    useState<UserMemoryDocumentId>("memory")
  const snapshot = useMemorySnapshot()
  const persistence = useMemorySave(snapshot)
  const reload = useCallback(() => {
    if (persistence.dirty && !window.confirm(t("discardChangesConfirm"))) return
    void snapshot.load()
  }, [persistence.dirty, snapshot, t])
  return {
    ...snapshot,
    ...persistence,
    activeDocumentId,
    setActiveDocumentId,
    reload,
  }
}
