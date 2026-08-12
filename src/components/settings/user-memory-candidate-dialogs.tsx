"use client"

import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
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
  const t = useTranslations("UserMemorySettings")
  const active = !TERMINAL_STATUSES.includes(candidate.status)
  const wordingVariantCount = candidate.wordingVariants?.length ?? 0
  return (
    <li className="rounded-md border bg-background/50 p-2">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="break-words leading-5">{candidate.content}</p>
          {wordingVariantCount > 0 && (
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              {t("diagnostics.candidates.variants", {
                count: wordingVariantCount,
              })}
            </p>
          )}
          <p className="mt-0.5 text-[10px] text-muted-foreground">
            {candidate.signal} ·{" "}
            {t("diagnostics.candidates.observationCount", {
              count: candidate.observationCount,
            })}{" "}
            ·{" "}
            {t("diagnostics.candidates.confidence", {
              value: candidate.confidence,
            })}
          </p>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          {candidate.status === "pending_confirmation" && (
            <Button
              size="sm"
              variant="default"
              disabled={busy}
              onClick={onConfirm}
            >
              {t("diagnostics.candidates.confirm")}
            </Button>
          )}
          {active && (
            <span className="flex gap-1">
              <Button
                size="sm"
                variant="outline"
                disabled={busy}
                onClick={onReject}
              >
                {t("diagnostics.candidates.reject")}
              </Button>
              {canMerge && (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy}
                  onClick={onMerge}
                >
                  {t("diagnostics.candidates.merge")}
                </Button>
              )}
            </span>
          )}
          {candidate.status === "confirmed" && (
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={onDelete}
            >
              {t("diagnostics.candidates.delete")}
            </Button>
          )}
        </div>
      </div>
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
  const t = useTranslations("UserMemorySettings")
  return (
    <Dialog
      open={candidate !== null}
      onOpenChange={(open) => !open && onClose()}
    >
      <DialogContent className="max-w-lg rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("diagnostics.candidates.confirmTitle")}</DialogTitle>
          <DialogDescription>
            {t("diagnostics.candidates.confirmDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <Label htmlFor="candidate-confirm-content">
            {t("diagnostics.candidates.edit")}
          </Label>
          <Textarea
            id="candidate-confirm-content"
            value={content}
            onChange={(event) => onContentChange(event.target.value)}
            className="min-h-24 resize-y"
          />
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={loading}
            onClick={onClose}
          >
            {t("diagnostics.candidates.cancel")}
          </Button>
          <Button
            type="button"
            disabled={loading || !candidate}
            onClick={onSubmit}
          >
            {loading && <Loader2 className="animate-spin" />}
            {t("diagnostics.candidates.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
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
  const t = useTranslations("UserMemorySettings")
  const targets = candidates.filter((item) =>
    isMergeTarget(item, candidate?.id)
  )
  const canMerge = candidate !== null && targets.length > 0
  return (
    <Dialog open={canMerge} onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-lg rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("diagnostics.candidates.mergeTitle")}</DialogTitle>
          <DialogDescription>
            {t("diagnostics.candidates.mergeDescription")}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-2">
          <Label htmlFor="candidate-merge-target">
            {t("diagnostics.candidates.mergeTarget")}
          </Label>
          <Select value={target} onValueChange={onTargetChange}>
            <SelectTrigger id="candidate-merge-target" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {targets.map((item) => (
                <SelectItem key={item.id} value={item.id}>
                  {item.content.slice(0, 60)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            disabled={loading}
            onClick={onClose}
          >
            {t("diagnostics.candidates.cancel")}
          </Button>
          <Button
            type="button"
            disabled={loading || !candidate || !target}
            onClick={onSubmit}
          >
            {loading && <Loader2 className="animate-spin" />}
            {t("diagnostics.candidates.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
