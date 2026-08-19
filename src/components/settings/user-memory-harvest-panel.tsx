"use client"

import { useCallback, useState } from "react"
import { Layers, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import type {
  UserMemoryHarvestStatus,
  UserMemoryHarvestRescanResult,
} from "@/lib/user-memory-documents"

interface UserMemoryHarvestPanelProps {
  harvest: UserMemoryHarvestStatus | null
  busy: boolean
  refresh: () => Promise<void>
  onError: (message: string) => void
}

function formatTime(value: string | null | undefined): string {
  if (!value) return ""
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString()
}

interface RescanApi {
  rescanUserMemoryHarvest?: (
    execute: boolean
  ) => Promise<UserMemoryHarvestRescanResult>
  rebuildUserMemoryCandidateIndex?: (execute: boolean) => Promise<{
    affected: number
    executed: boolean
    revision: string
  }>
}

export function UserMemoryHarvestPanel({
  harvest,
  busy,
  refresh,
  onError,
}: UserMemoryHarvestPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const [loading, setLoading] = useState(false)

  const runRescan = useCallback(async () => {
    const apiModule = await import("@/lib/api")
    const rescan = (apiModule as RescanApi).rescanUserMemoryHarvest
    if (typeof rescan !== "function") return
    setLoading(true)
    try {
      await rescan(true)
      await refresh()
      toast.success(t("diagnostics.rescanDone"))
    } catch (error) {
      onError(toErrorMessage(error))
    } finally {
      setLoading(false)
    }
  }, [onError, refresh, t])

  const runRebuildIndex = useCallback(async () => {
    const apiModule = await import("@/lib/api")
    const rebuild = (apiModule as RescanApi).rebuildUserMemoryCandidateIndex
    if (typeof rebuild !== "function") return
    setLoading(true)
    try {
      await rebuild(true)
      await refresh()
      toast.success(t("diagnostics.rebuildDone"))
    } catch (error) {
      onError(toErrorMessage(error))
    } finally {
      setLoading(false)
    }
  }, [onError, refresh, t])

  return (
    <div className="border-t pt-3 text-xs">
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 font-medium">
          <Layers className="h-3.5 w-3.5 text-muted-foreground" aria-hidden />
          {t("diagnostics.harvest")}
        </span>
        <span className="flex gap-2">
          <Button
            size="sm"
            variant="outline"
            disabled={busy || loading}
            onClick={() => void runRescan()}
          >
            <Loader2 className={loading ? "animate-spin" : "hidden"} />
            {t("diagnostics.rescan")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={busy || loading}
            onClick={() => void runRebuildIndex()}
          >
            {t("diagnostics.rebuildIndex")}
          </Button>
        </span>
      </div>
      {harvest ? (
        <>
          <p className="flex flex-wrap gap-x-3 gap-y-1">
            <span>
              {t("diagnostics.harvestQueued", { queued: harvest.queued })}
            </span>
            <span>
              {t("diagnostics.harvestExtracting", {
                extracting: harvest.extracting,
              })}
            </span>
            <span>
              {t("diagnostics.harvestProposed", { proposed: harvest.proposed })}
            </span>
            <span>{t("diagnostics.harvestNoop", { noop: harvest.noop })}</span>
            <span>
              {t("diagnostics.harvestFailed", { failed: harvest.failed })}
            </span>
            <span>{t("diagnostics.harvestDead", { dead: harvest.dead })}</span>
            <span>
              {t("diagnostics.harvestBacklog", { backlog: harvest.backlog })}
            </span>
          </p>
          <p className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-muted-foreground">
            {harvest.lastHarvestAt && (
              <span>
                {t("diagnostics.harvestLast", {
                  time: formatTime(harvest.lastHarvestAt),
                })}
              </span>
            )}
            {harvest.lastSuccessWriteAt && (
              <span>
                {t("diagnostics.harvestLastSuccess", {
                  time: formatTime(harvest.lastSuccessWriteAt),
                })}
              </span>
            )}
            {harvest.lastFailureAt && (
              <span className="text-red-400">
                {t("diagnostics.harvestLastFailure", {
                  time: formatTime(harvest.lastFailureAt),
                })}
              </span>
            )}
            {!harvest.lastHarvestAt && !harvest.lastFailureAt && (
              <span>{t("diagnostics.harvestNever")}</span>
            )}
          </p>
        </>
      ) : (
        <p className="text-muted-foreground">{t("diagnostics.harvestNever")}</p>
      )}
    </div>
  )
}
