"use client"

import { useMemo } from "react"
import { Bot, Check, CircleDot, SkipForward } from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { getAgentLabel } from "@/lib/custom-agents"
import {
  type UserMemoryCandidateStatus,
  type UserMemoryCandidateSummary,
} from "@/lib/user-memory-documents"

interface UserMemoryCandidatesPanelProps {
  candidates: UserMemoryCandidateSummary[]
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

function statusIcon(status: UserMemoryCandidateStatus) {
  if (status === "confirmed") return Check
  if (status === "rejected" || status === "superseded") return SkipForward
  return CircleDot
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
      <div className="flex items-start justify-between gap-3 border-b px-4 py-3">
        <div className="flex min-w-0 gap-2">
          <Bot
            className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground"
            aria-hidden
          />
          <div>
            <h2 className="text-sm font-semibold">
              {t("diagnostics.candidates.title")}
            </h2>
            <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
              {t("diagnostics.candidates.activityDescription")}
            </p>
          </div>
        </div>
        <Badge variant="outline" className="shrink-0 text-[10px]">
          {t("diagnostics.candidates.total", { count: candidates.length })}
        </Badge>
      </div>

      {activities.length === 0 ? (
        <p className="px-4 py-5 text-xs text-muted-foreground">
          {t("diagnostics.candidates.empty")}
        </p>
      ) : (
        <ul className="max-h-80 divide-y overflow-y-auto">
          {activities.map((candidate) => {
            const StatusIcon = statusIcon(candidate.status)
            const sources = candidate.sourceAgents
              .map(getAgentLabel)
              .join(" · ")
            const active = ACTIVE_STATUSES.includes(candidate.status)
            return (
              <li key={candidate.id} className="px-4 py-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="break-words text-sm leading-5">
                      {candidate.content}
                    </p>
                    <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
                      {candidate.signal} ·{" "}
                      {t("diagnostics.candidates.observationCount", {
                        count: candidate.observationCount,
                      })}
                      {` · ${t("diagnostics.candidates.confidence", {
                        value: candidate.confidence,
                      })}`}
                      {sources ? ` · ${sources}` : ""}
                    </p>
                    <p className="text-[11px] text-muted-foreground">
                      {formatTime(candidate.lastObservedAt)}
                    </p>
                  </div>
                  <span
                    className={`flex shrink-0 items-center gap-1 text-[11px] font-medium ${statusTone(candidate.status)}`}
                  >
                    <StatusIcon
                      className={active ? "h-3 w-3 animate-pulse" : "h-3 w-3"}
                      aria-hidden
                    />
                    {t(`diagnostics.candidates.${candidate.status}`)}
                  </span>
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}
