import type {
  UserMemoryCandidateStatus,
  UserMemoryCandidateSummary,
} from "@/lib/user-memory-documents"

const TERMINAL_STATUSES: UserMemoryCandidateStatus[] = [
  "confirmed",
  "rejected",
  "superseded",
]

export function isMergeTarget(
  candidate: UserMemoryCandidateSummary,
  sourceId: string | undefined
): boolean {
  return (
    candidate.id !== sourceId && !TERMINAL_STATUSES.includes(candidate.status)
  )
}

export function CandidateRow({
  candidate,
  busy,
  onConfirm,
  onReject,
  onMerge,
  canMerge,
  onDelete,
}: {
  candidate: UserMemoryCandidateSummary
  busy: boolean
  onConfirm: () => void
  onReject: () => void
  onMerge: () => void
  canMerge: boolean
  onDelete: () => void
}) {
  void busy
  void onConfirm
  void onReject
  void onMerge
  void canMerge
  void onDelete
  return (
    <li className="rounded-md border bg-background/50 p-2">
      <p className="break-words leading-5">{candidate.content}</p>
      <p className="mt-0.5 text-[10px] text-muted-foreground">
        {candidate.signal} · {candidate.observationCount} observations ·{" "}
        {candidate.confidence}%
      </p>
    </li>
  )
}

export function ConfirmDialog({
  candidate,
  content,
  loading,
  onContentChange,
  onClose,
  onSubmit,
}: {
  candidate: UserMemoryCandidateSummary | null
  content: string
  loading: boolean
  onContentChange: (value: string) => void
  onClose: () => void
  onSubmit: () => void
}) {
  void candidate
  void content
  void loading
  void onContentChange
  void onClose
  void onSubmit
  return null
}

export function MergeDialog({
  candidate,
  candidates,
  target,
  loading,
  onTargetChange,
  onClose,
  onSubmit,
}: {
  candidate: UserMemoryCandidateSummary | null
  candidates: UserMemoryCandidateSummary[]
  target: string
  loading: boolean
  onTargetChange: (value: string) => void
  onClose: () => void
  onSubmit: () => void
}) {
  void candidate
  void candidates
  void target
  void loading
  void onTargetChange
  void onClose
  void onSubmit
  return null
}
