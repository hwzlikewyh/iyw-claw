"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { toErrorMessage } from "@/lib/app-error"
import {
  clearUserMemory,
  listUserMemoryEntries,
  type ClearUserMemoryScope,
} from "@/lib/user-memory-entries"

function useClearRevision(open: boolean, generation: number) {
  const [response, setResponse] = useState<{
    generation: number
    revision: string | null
    error: string | null
  } | null>(null)
  useEffect(() => {
    if (!open) return
    let current = true
    listUserMemoryEntries({
      document: "memory",
      query: "",
      offset: 0,
      includeInactive: true,
    })
      .then((page) => {
        if (current)
          setResponse({ generation, revision: page.revision, error: null })
      })
      .catch((reason) => {
        if (current)
          setResponse({
            generation,
            revision: null,
            error: toErrorMessage(reason),
          })
      })
    return () => {
      current = false
    }
  }, [open, generation])
  return response?.generation === generation ? response : null
}

function useClearAction(onUpdated: () => void) {
  const t = useTranslations("UserMemorySettings.clear")
  const pending = useRef(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = async (request: Parameters<typeof clearUserMemory>[0]) => {
    if (pending.current) return false
    pending.current = true
    setBusy(true)
    setError(null)
    try {
      const result = await clearUserMemory(request)
      const residual =
        result.residualBackupPaths.length + result.residualFilePaths.length
      const description = residual
        ? t("residual", { count: residual })
        : t("complete")
      if (residual) toast.warning(t("cleared"), { description })
      else toast.success(t("cleared"), { description })
      onUpdated()
      return true
    } catch (reason) {
      setError(toErrorMessage(reason))
      return false
    } finally {
      pending.current = false
      setBusy(false)
    }
  }
  return { busy, pending, error, setError, submit }
}

function useClearChoices() {
  const [scope, setScope] = useState<ClearUserMemoryScope>("memory")
  const [purgeBackups, setPurgeBackups] = useState(false)
  const [confirmed, setConfirmed] = useState(false)
  const changeScope = (value: ClearUserMemoryScope) => {
    setScope(value)
    setConfirmed(false)
  }
  const reset = () => {
    changeScope("memory")
    setPurgeBackups(false)
  }
  return {
    scope,
    changeScope,
    purgeBackups,
    setPurgeBackups,
    confirmed,
    setConfirmed,
    reset,
  }
}

export function useClearUserMemory(onUpdated: () => void) {
  const [open, setOpen] = useState(false)
  const choices = useClearChoices()
  const [generation, setGeneration] = useState(0)
  const response = useClearRevision(open, generation)
  const action = useClearAction(onUpdated)
  const refresh = () => {
    choices.setConfirmed(false)
    action.setError(null)
    setGeneration((value) => value + 1)
  }
  const changeOpen = (value: boolean) => {
    if (action.pending.current) return
    choices.reset()
    refresh()
    setOpen(value)
  }
  const submit = async () => {
    if (!choices.confirmed || !response?.revision) return
    const success = await action.submit({
      scope: choices.scope,
      purgeBackups: choices.purgeBackups,
      expectedRevision: response.revision,
      confirmation: "CLEAR",
    })
    if (success) setOpen(false)
  }
  return {
    ...choices,
    open,
    changeOpen,
    refresh,
    submit,
    busy: action.busy,
    loading: open && response === null,
    ready: !!response?.revision,
    error: action.error ?? response?.error,
  }
}
