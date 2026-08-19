"use client"

import { useCallback, useEffect, useState } from "react"
import { Activity, Database, HeartPulse } from "lucide-react"
import { useTranslations } from "next-intl"

import { toErrorMessage } from "@/lib/app-error"
import type {
  UserMemoryCapabilities,
  UserMemoryCandidateListRequest,
  UserMemoryCandidatePage,
  UserMemoryCandidateSummary,
  UserMemoryHarvestStatus,
  UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"
import { UserMemoryHarvestPanel } from "./user-memory-harvest-panel"
import { UserMemoryCandidatesPanel } from "./user-memory-candidates-panel"

interface UserMemoryDiagnosticsPanelProps {
  settings: UserMemorySettingsSnapshot
  busy: boolean
}

const CANDIDATE_PAGE_SIZE = 100

type CandidateList = (
  request: UserMemoryCandidateListRequest
) => Promise<UserMemoryCandidatePage>

async function loadAllCandidates(
  list: CandidateList,
  retryOnChange = true
): Promise<UserMemoryCandidatePage> {
  const candidates: UserMemoryCandidateSummary[] = []
  let revision: string | null = null
  let total = 0
  while (candidates.length < total || revision === null) {
    const page = await list({
      status: null,
      offset: candidates.length,
      limit: CANDIDATE_PAGE_SIZE,
    })
    if (revision !== null && page.revision !== revision) {
      if (retryOnChange) return loadAllCandidates(list, false)
      throw new Error("User memory candidates changed while loading")
    }
    revision = page.revision
    total = page.total
    candidates.push(...page.candidates)
    if (page.candidates.length === 0) break
  }
  return {
    candidates,
    total,
    offset: 0,
    limit: CANDIDATE_PAGE_SIZE,
    revision: revision ?? "",
  }
}

export function UserMemoryDiagnosticsPanel({
  settings,
  busy,
}: UserMemoryDiagnosticsPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const [candidates, setCandidates] = useState<UserMemoryCandidateSummary[]>([])
  const [harvest, setHarvest] = useState<UserMemoryHarvestStatus | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)

  const loadState = useCallback(async () => {
    const apiModule = await import("@/lib/api")
    const list = apiModule.listUserMemoryCandidates
    const status = apiModule.getUserMemoryHarvestStatus
    if (typeof list === "function") {
      try {
        const page = await loadAllCandidates(list)
        setCandidates(page.candidates)
      } catch (error) {
        setActionError(toErrorMessage(error))
      }
    }
    if (typeof status === "function") {
      try {
        setHarvest(await status())
      } catch (error) {
        setActionError(toErrorMessage(error))
      }
    }
  }, [])

  useEffect(() => {
    void loadState()
  }, [loadState])

  const refreshHarvest = useCallback(async () => {
    const apiModule = await import("@/lib/api")
    const status = apiModule.getUserMemoryHarvestStatus
    if (typeof status !== "function") return
    try {
      setHarvest(await status())
    } catch (error) {
      setActionError(toErrorMessage(error))
    }
  }, [])

  return (
    <section className="space-y-3">
      <UserMemoryCandidatesPanel candidates={candidates} />

      {actionError && (
        <div
          role="alert"
          className="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-1.5 text-xs text-red-400"
        >
          {actionError}
        </div>
      )}

      <details className="overflow-hidden rounded-xl border bg-card">
        <summary className="cursor-pointer list-none px-4 py-3 marker:hidden">
          <span className="flex items-start gap-2">
            <Activity
              className="mt-0.5 h-4 w-4 text-muted-foreground"
              aria-hidden
            />
            <span>
              <span className="block text-sm font-semibold">
                {t("diagnostics.title")}
              </span>
              <span className="mt-0.5 block text-xs leading-5 text-muted-foreground">
                {t("diagnostics.description")}
              </span>
            </span>
          </span>
        </summary>

        <div className="space-y-4 border-t px-4 py-3">
          <div className="grid gap-4 md:grid-cols-2">
            <StorageDiagnostics settings={settings} />
            <CompanionDiagnostics
              companion={settings.companionHealth}
              capabilities={settings.projectedCapabilities?.codex}
            />
          </div>

          <UserMemoryHarvestPanel
            harvest={harvest}
            busy={busy}
            refresh={refreshHarvest}
            onError={setActionError}
          />
        </div>
      </details>
    </section>
  )
}

function StorageDiagnostics({
  settings,
}: {
  settings: UserMemorySettingsSnapshot
}) {
  const t = useTranslations("UserMemorySettings")
  const availability = settings.availability
  const candidateDiagnostic = settings.candidateDiagnostic
  return (
    <div className="min-w-0 text-xs">
      <div className="mb-1 flex items-center gap-1.5 font-medium">
        <Database className="h-3.5 w-3.5 text-muted-foreground" aria-hidden />
        {t("diagnostics.root")}
      </div>
      <p className="break-all font-mono text-[11px] leading-5">
        {settings.resolvedRoot ?? t("diagnostics.unavailable")}
      </p>
      <p className="mt-1 text-[11px] text-muted-foreground">
        {t("diagnostics.rootSource")}: {settings.rootSource ?? "—"}
      </p>
      <p className="mt-1">
        {t("diagnostics.availability")}:{" "}
        {availability?.available
          ? t("diagnostics.available")
          : t("diagnostics.unavailable")}
        {!availability?.available && availability?.detail
          ? ` — ${availability.detail}`
          : ""}
      </p>
      <p className="mt-1">
        {t("diagnostics.candidateState")}:{" "}
        {candidateDiagnostic?.available
          ? t("diagnostics.available")
          : t("diagnostics.unavailable")}
        {!candidateDiagnostic?.available && candidateDiagnostic?.detail
          ? ` — ${candidateDiagnostic.detail}`
          : ""}
      </p>
      {settings.migrationReport &&
        (settings.migrationReport.warnings.length > 0 ? (
          <p className="mt-1 text-amber-500">
            {t("diagnostics.migrationWarnings", {
              count: settings.migrationReport.warnings.length,
            })}
          </p>
        ) : (
          <p className="mt-1">{t("diagnostics.noWarnings")}</p>
        ))}
    </div>
  )
}

function CompanionDiagnostics({
  companion,
  capabilities,
}: {
  companion: UserMemorySettingsSnapshot["companionHealth"]
  capabilities: UserMemoryCapabilities | undefined
}) {
  const t = useTranslations("UserMemorySettings")
  const isReady = companion?.status === "ready"
  return (
    <div className="min-w-0 text-xs">
      <div className="mb-1 flex items-center gap-1.5 font-medium">
        <HeartPulse className="h-3.5 w-3.5 text-muted-foreground" aria-hidden />
        {t("diagnostics.companion")}
      </div>
      <p>
        {isReady ? t("diagnostics.companionReady") : (companion?.status ?? "—")}
        {!isReady && companion?.detail ? ` — ${companion.detail}` : ""}
      </p>
      <p className="mt-1">
        {companion?.detectedVersion
          ? t("diagnostics.companionVersion", {
              expected: companion.expectedVersion,
              detected: companion.detectedVersion,
            })
          : companion
            ? t("diagnostics.companionNoVersion", {
                expected: companion.expectedVersion,
              })
            : "—"}
      </p>
      <p className="mt-1">
        {t("diagnostics.companionTools", {
          count: companion?.advertisedTools.length ?? 0,
        })}
      </p>
      {capabilities && (
        <p className="mt-1">
          {t("diagnostics.capabilities")}:{" "}
          {[
            capabilities.readContext.available &&
              t("diagnostics.capabilityRead"),
            capabilities.confirmedAppend.available &&
              t("diagnostics.capabilityAppend"),
            capabilities.candidateProposal.available &&
              t("diagnostics.capabilityPropose"),
          ]
            .filter(Boolean)
            .join(" · ") || t("diagnostics.capabilityNo")}
        </p>
      )}
    </div>
  )
}
