"use client"

import { useEffect, useState } from "react"
import { toast } from "sonner"
import { useTranslations } from "next-intl"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemoryReconciliation,
  type MemoryReconciliation,
  type ReconciliationResult,
} from "@/lib/user-memory-reconcile"

export function useMemoryReconciliation(onUpdated: () => void) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const [preview, setPreview] = useState<MemoryReconciliation | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let current = true
    getMemoryReconciliation()
      .then((value) => current && setPreview(value))
      .catch((reason) => current && setError(toErrorMessage(reason)))
    return () => {
      current = false
    }
  }, [])
  const refresh = async () => {
    setBusy(true)
    setError(null)
    try {
      setPreview(await getMemoryReconciliation())
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  const apply = async (action: () => Promise<ReconciliationResult>) => {
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      const result = await action()
      setPreview(await getMemoryReconciliation())
      toast.success(t("resolved"), { description: result.backupPath })
      onUpdated()
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  return { preview, busy, error, refresh, apply }
}
