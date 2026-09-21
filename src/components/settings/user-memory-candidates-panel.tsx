"use client"

import { useMemo, useState } from "react"
import { Bot, Check, CircleDot, SkipForward, X } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { resolveUserMemoryCandidate } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import {
  type UserMemoryCandidateStatus,
  type UserMemoryCandidateSummary,
} from "@/lib/user-memory-documents"

interface UserMemoryCandidatesPanelProps {
  candidates: UserMemoryCandidateSummary[]
  revision: string | null
  onChanged: () => void
}

const ACTIVE_STATUSES: UserMemoryCandidateStatus[] = [
  "tentative",
  "emerging",
  "pending_confirmation",
]

function formatTime(value: string): string {
  const parsed = new Date(value)
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString()
}

function CandidateStatusIcon({
  status,
}: {
  status: UserMemoryCandidateStatus
}) {
  if (status === "confirmed") return <Check className="h-3 w-3" aria-hidden />
  if (status === "rejected" || status === "superseded")
    return <SkipForward className="h-3 w-3" aria-hidden />
  return <CircleDot className="h-3 w-3 animate-pulse" aria-hidden />
}

function statusTone(status: UserMemoryCandidateStatus): string {
  if (status === "confirmed") return "text-emerald-500"
  if (status === "rejected" || status === "superseded") {
    return "text-muted-foreground"
  }
  return "text-amber-500"
}

export function UserMemoryCandidatesPanel({
  candidates,
  revision,
  onChanged,
}: UserMemoryCandidatesPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const activities = useMemo(
    () =>
      [...candidates].sort(
        (left, right) =>
          Date.parse(right.lastObservedAt) - Date.parse(left.lastObservedAt)
      ),
    [candidates]
  )

  return (
    <div className="overflow-hidden rounded-xl border bg-card">
      <CandidateHeader count={candidates.length} />
      {activities.length === 0 ? (
        <p className="px-4 py-5 text-xs text-muted-foreground">
          {t("diagnostics.candidates.empty")}
        </p>
      ) : (
        <ul className="max-h-80 divide-y overflow-y-auto">
          {activities.map((candidate) => (
            <CandidateRow
              key={candidate.id}
              candidate={candidate}
              revision={revision}
              onChanged={onChanged}
            />
          ))}
        </ul>
      )}
    </div>
  )
}

function CandidateHeader({ count }: { count: number }) {
  const t = useTranslations("UserMemorySettings.diagnostics.candidates")
  return (
    <div className="flex items-start justify-between gap-3 border-b px-4 py-3">
      <div className="flex min-w-0 gap-2">
        <Bot
          className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
          aria-hidden
        />
        <div>
          <h2 className="text-sm font-semibold">{t("title")}</h2>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            {t("activityDescription")}
          </p>
        </div>
      </div>
      <Badge variant="outline" className="shrink-0 text-[10px]">
        {count} {t("totalLabel")}
      </Badge>
    </div>
  )
}

interface CandidateRowProps {
  candidate: UserMemoryCandidateSummary
  revision: string | null
  onChanged: () => void
}

function CandidateRow({ candidate, revision, onChanged }: CandidateRowProps) {
  const t = useTranslations("UserMemorySettings.diagnostics.candidates")
  const active = ACTIVE_STATUSES.includes(candidate.status)
  return (
    <li className="px-4 py-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 flex-1 basis-48">
          <p className="break-words text-sm leading-5">{candidate.content}</p>
          <CandidateDetails candidate={candidate} />
        </div>
        <span
          className={`flex shrink-0 items-center gap-1 text-[11px] font-medium ${statusTone(candidate.status)}`}
        >
          <CandidateStatusIcon status={candidate.status} />
          {t(candidate.status)}
        </span>
      </div>
      {active && revision && (
        <CandidateActions
          candidate={candidate}
          revision={revision}
          onChanged={onChanged}
        />
      )}
    </li>
  )
}

function CandidateDetails({
  candidate,
}: {
  candidate: UserMemoryCandidateSummary
}) {
  const t = useTranslations("UserMemorySettings.diagnostics.candidates")
  const sources = candidate.sourceAgents.join(" · ")
  return (
    <>
      <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
        {candidate.signal} · {candidate.observationCount}{" "}
        {t("observationsLabel")}
        {` · ${t("confidenceLabel")} ${candidate.confidence}%`}
        {sources ? ` · ${sources}` : ""}
      </p>
      <p className="text-[11px] text-muted-foreground">
        {formatTime(candidate.lastObservedAt)}
      </p>
    </>
  )
}

function useCandidateResolution({
  candidate,
  revision,
  onChanged,
}: {
  candidate: UserMemoryCandidateSummary
  revision: string
  onChanged: () => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const resolve = async (confirm: boolean) => {
    setBusy(true)
    setError(null)
    try {
      await resolveUserMemoryCandidate({
        candidateId: candidate.id,
        expectedRevision: revision,
        resolution: confirm
          ? { type: "confirm", editedContent: null }
          : { type: "reject" },
      })
      onChanged()
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      setBusy(false)
    }
  }
  return { busy, error, resolve }
}

function CandidateActions(props: CandidateRowProps & { revision: string }) {
  const t = useTranslations("UserMemorySettings.entries")
  const { busy, error, resolve } = useCandidateResolution(props)
  return (
    <div className="mt-2 flex flex-wrap items-center gap-1">
      <Button
        size="icon"
        variant="ghost"
        title={t("confirmCandidate")}
        aria-label={t("confirmCandidate")}
        disabled={busy}
        onClick={() => void resolve(true)}
      >
        <Check className="size-4" />
      </Button>
      <Button
        size="icon"
        variant="ghost"
        title={t("rejectCandidate")}
        aria-label={t("rejectCandidate")}
        disabled={busy}
        onClick={() => void resolve(false)}
      >
        <X className="size-4" />
      </Button>
      {error && (
        <p role="alert" className="w-full break-words text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}
