import { useCallback, useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  getMemoryMaintenance,
  previewMemoryMigration,
  reconcileMemoryMigration,
  resolveMemoryReview,
  runMemoryMaintenance,
  type MemoryMaintenanceStatus,
  type MemoryMigrationPreview,
} from "@/lib/user-memory-maintenance"

const STATUS_INTERVAL_MS = 30_000

function useMaintenanceStatus() {
  const [status, setStatus] = useState<MemoryMaintenanceStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const refresh = useCallback(async () => {
    try {
      setStatus(await getMemoryMaintenance())
    } catch (reason) {
      setError(toErrorMessage(reason))
    }
  }, [])
  useEffect(() => {
    const initial = setTimeout(() => void refresh(), 0)
    const timer = setInterval(() => void refresh(), STATUS_INTERVAL_MS)
    return () => {
      clearTimeout(initial)
      clearInterval(timer)
    }
  }, [refresh])
  return { status, refresh, error, setError }
}

export function useMemoryMaintenance(onUpdated: () => void) {
  const { status, refresh, error, setError } = useMaintenanceStatus()
  const [preview, setPreview] = useState<MemoryMigrationPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const execute = async (operation: () => Promise<unknown>) => {
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      await operation()
      await refresh()
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  const resolve = (id: string, apply: boolean) =>
    execute(async () => {
      if (!status) return
      await resolveMemoryReview({
        id,
        apply,
        expectedRevision: status.revision,
      })
      onUpdated()
    })
  const reconcilePreview = () =>
    execute(async () => {
      if (!preview) return
      await reconcileMemoryMigration(preview.sourceRevision)
      setPreview(await previewMemoryMigration())
      onUpdated()
    })
  return {
    status,
    preview,
    setPreview,
    busy,
    error,
    refresh,
    resolve,
    run: () => execute(runMemoryMaintenance),
    loadPreview: () =>
      execute(async () => setPreview(await previewMemoryMigration())),
    reconcilePreview,
  }
}
