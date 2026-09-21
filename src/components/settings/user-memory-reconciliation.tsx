"use client"

import { useState } from "react"
import { Loader2, RefreshCw, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { restoreMemoryAuthority } from "@/lib/user-memory-reconcile"
import { useMemoryReconciliation } from "./use-memory-reconciliation"
import { MemoryConflictFile } from "./user-memory-conflict-file"

type State = ReturnType<typeof useMemoryReconciliation>

function RecoveryChoices({
  state,
  selected,
  onSelect,
}: {
  state: State
  selected: string
  onSelect: (id: string) => void
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  return (
    <fieldset disabled={state.busy} className="space-y-2">
      <legend className="mb-2 text-sm font-medium">{t("sources")}</legend>
      {state.preview!.recoverySources.map((source) => (
        <label
          key={source.id}
          className="flex items-start gap-2 border-b py-2 text-xs"
        >
          <input
            type="radio"
            name="memory-recovery"
            checked={selected === source.id}
            onChange={() => onSelect(source.id)}
          />
          <span className="min-w-0 break-all">
            {source.label} · v{source.epoch}
            <br />
            {t("records")}: {source.records} · {t("revisions")}:{" "}
            {source.revisions}
          </span>
        </label>
      ))}
    </fieldset>
  )
}

export function UserMemoryReconciliationPanel({
  onUpdated,
}: {
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const state = useMemoryReconciliation(onUpdated)
  return (
    <section className="w-full min-w-0 space-y-3 border-b py-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="text-sm font-medium">{t("title")}</h2>
        <Button
          size="icon"
          variant="ghost"
          title={t("refresh")}
          aria-label={t("refresh")}
          disabled={state.busy}
          onClick={() => void state.refresh()}
        >
          {state.busy ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <RefreshCw className="size-4" />
          )}
        </Button>
      </div>
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
      {state.preview ? (
        <ReconciliationContent state={state} />
      ) : (
        !state.error && (
          <p role="status" className="text-xs text-muted-foreground">
            {t("loading")}
          </p>
        )
      )}
    </section>
  )
}

function ReconciliationContent({ state }: { state: State }) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const preview = state.preview!
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">{t(preview.mode)}</p>
      {preview.files.map((file) => (
        <MemoryConflictFile
          key={`${preview.revision}:${file.name}`}
          file={file}
          state={state}
        />
      ))}
      {preview.mode === "restore_required" && (
        <RecoverySources key={preview.revision} state={state} />
      )}
    </div>
  )
}

function RecoverySources({ state }: { state: State }) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const preview = state.preview!
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">
        {t("databaseEpoch")}: {preview.databaseEpoch ?? "-"} ·{" "}
        {t("requiredEpoch")}: {preview.requiredEpoch ?? "-"}
      </p>
      {preview.recoverySources.length ? (
        <RecoveryForm state={state} />
      ) : (
        <p role="alert" className="text-xs text-amber-700 dark:text-amber-400">
          {t("noEvidence")}
        </p>
      )}
    </div>
  )
}

function RecoveryForm({ state }: { state: State }) {
  const t = useTranslations("UserMemorySettings.reconcile")
  const preview = state.preview!
  const [selected, setSelected] = useState("")
  const [acknowledged, setAcknowledged] = useState(false)
  const restore = () =>
    state.apply(() =>
      restoreMemoryAuthority({
        expectedRevision: preview.revision,
        sourceId: selected,
      })
    )
  return (
    <div className="space-y-3">
      <>
        <RecoveryChoices
          state={state}
          selected={selected}
          onSelect={(id) => {
            setSelected(id)
            setAcknowledged(false)
          }}
        />
        <label className="flex items-start gap-2 text-xs">
          <input
            type="checkbox"
            checked={acknowledged}
            disabled={state.busy || !selected}
            onChange={(event) => setAcknowledged(event.target.checked)}
          />
          {t("acknowledge")}
        </label>
        <Button
          size="sm"
          disabled={state.busy || !selected || !acknowledged}
          onClick={() => void restore()}
        >
          <RotateCcw className="size-4" />
          {t("restore")}
        </Button>
      </>
    </div>
  )
}
