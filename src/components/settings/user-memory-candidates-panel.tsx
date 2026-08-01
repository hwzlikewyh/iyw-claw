"use client"

import { useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { toErrorMessage } from "@/lib/app-error"
import {
  USER_MEMORY_CANDIDATE_STATUS_ORDER,
  type UserMemoryCandidateResolveRequest,
  type UserMemoryCandidateStatus,
  type UserMemoryCandidateSummary,
  type UserMemorySettingsSnapshot,
} from "@/lib/user-memory-documents"
import { CandidateRow, ConfirmDialog, MergeDialog } from "./user-memory-candidate-dialogs"

interface UserMemoryCandidatesPanelProps {
  settings: UserMemorySettingsSnapshot
  candidates: UserMemoryCandidateSummary[]
  revision: string | null
  busy: boolean
  onChanged: () => Promise<void>
  onError: (message: string) => void
}

export function UserMemoryCandidatesPanel({
  settings,
  candidates,
  revision,
  busy,
  onChanged,
  onError,
}: UserMemoryCandidatesPanelProps) {
  const t = useTranslations("UserMemorySettings")
  const [loading, setLoading] = useState(false)
  const [confirmCandidate, setConfirmCandidate] =
    useState<UserMemoryCandidateSummary | null>(null)
  const [editedContent, setEditedContent] = useState("")
  const [mergeCandidate, setMergeCandidate] =
    useState<UserMemoryCandidateSummary | null>(null)
  const [mergeTarget, setMergeTarget] = useState("")

  const grouped = useMemo(() => {
    const groups = new Map<UserMemoryCandidateStatus, UserMemoryCandidateSummary[]>()
    for (const status of USER_MEMORY_CANDIDATE_STATUS_ORDER) groups.set(status, [])
    for (const candidate of candidates) {
      groups.get(candidate.status)?.push(candidate)
    }
    return groups
  }, [candidates])

  async function resolve(
    candidate: UserMemoryCandidateSummary,
    resolution: UserMemoryCandidateResolveRequest["resolution"]
  ) {
    if (!revision) return
    const module = await import("@/lib/api")
    const call = (module as { resolveUserMemoryCandidate?: (request: UserMemoryCandidateResolveRequest) => Promise<unknown> })
      .resolveUserMemoryCandidate
    if (typeof call !== "function") return
    setLoading(true)
    try {
      await call({ candidateId: candidate.id, expectedRevision: revision, resolution })
      toast.success(t("diagnostics.candidates.done"))
      await onChanged()
    } catch (error) {
      onError(toErrorMessage(error))
    } finally {
      setLoading(false)
      setConfirmCandidate(null)
      setMergeCandidate(null)
    }
  }

  async function remove(candidate: UserMemoryCandidateSummary) {
    if (!revision) return
    const module = await import("@/lib/api")
    const call = (module as { deleteUserMemoryCandidate?: (request: { candidateId: string; expectedRevision: string }) => Promise<unknown> })
      .deleteUserMemoryCandidate
    if (typeof call !== "function") return
    setLoading(true)
    try {
      await call({ candidateId: candidate.id, expectedRevision: revision })
      toast.success(t("diagnostics.candidates.done"))
      await onChanged()
    } catch (error) {
      onError(toErrorMessage(error))
    } finally {
      setLoading(false)
    }
  }

  const counts = settings.candidateCounts ?? {}

  return (
    <div className="rounded-md border bg-muted/20 p-3 text-xs">
      <div className="mb-2 flex items-center justify-between">
        <span className="font-medium">{t("diagnostics.candidates.title")}</span>
        <span className="text-muted-foreground">
          {t("diagnostics.candidates.total", { count: candidates.length })}
        </span>
      </div>
      {candidates.length === 0 ? (
        <p className="text-muted-foreground">{t("diagnostics.candidates.empty")}</p>
      ) : (
        <div className="space-y-3">
          {USER_MEMORY_CANDIDATE_STATUS_ORDER.map((status) => {
            const list = grouped.get(status) ?? []
            if (list.length === 0) return null
            return (
              <div key={status}>
                <div className="mb-1 flex items-center gap-2">
                  <span className="font-medium">{t(`diagnostics.candidates.${status}`)}</span>
                  <Badge variant="outline" className="text-[10px]">
                    {list.length}
                  </Badge>
                  {(counts[status] ?? 0) > 0 && (
                    <span className="text-[10px] text-muted-foreground">{counts[status]}</span>
                  )}
                </div>
                <ul className="space-y-2">
                  {list.map((candidate) => (
                    <CandidateRow
                      key={candidate.id}
                      candidate={candidate}
                      busy={busy || loading}
                      onConfirm={() => {
                        setEditedContent(candidate.content)
                        setConfirmCandidate(candidate)
                      }}
                      onReject={() => void resolve(candidate, { type: "reject" })}
                      onMerge={() => {
                        setMergeTarget("")
                        setMergeCandidate(candidate)
                      }}
                      onDelete={() => void remove(candidate)}
                    />
                  ))}
                </ul>
              </div>
            )
          })}
        </div>
      )}

      <ConfirmDialog
        candidate={confirmCandidate}
        content={editedContent}
        loading={loading}
        onContentChange={setEditedContent}
        onClose={() => setConfirmCandidate(null)}
        onSubmit={() => {
          if (!confirmCandidate) return
          void resolve(confirmCandidate, {
            type: "confirm",
            editedContent: editedContent.trim(),
          })
        }}
      />

      <MergeDialog
        candidate={mergeCandidate}
        candidates={candidates}
        target={mergeTarget}
        loading={loading}
        onTargetChange={setMergeTarget}
        onClose={() => setMergeCandidate(null)}
        onSubmit={() => {
          if (!mergeCandidate || !mergeTarget) return
          void resolve(mergeCandidate, {
            type: "supersede_by_candidate",
            candidateId: mergeTarget,
          })
        }}
      />
    </div>
  )
}
