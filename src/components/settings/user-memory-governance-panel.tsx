"use client"

import { useEffect, useState } from "react"
import { Check, Loader2, RefreshCw, ShieldCheck } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { toErrorMessage } from "@/lib/app-error"
import {
  applyMemoryGovernance,
  previewMemoryGovernance,
  type MemoryGovernancePreview,
} from "@/lib/user-memory-governance"

export function UserMemoryGovernancePanel({
  disabled,
  onUpdated,
}: {
  disabled: boolean
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.governance")
  const state = useGovernance(onUpdated)
  return (
    <section className="min-w-0 space-y-3 border-b py-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2">
          <ShieldCheck className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
          <div>
            <h2 className="text-sm font-medium">{t("title")}</h2>
            <p className="text-xs leading-5 text-muted-foreground">
              {t("description")}
            </p>
          </div>
        </div>
        <Button
          size="icon"
          variant="ghost"
          title={t("refresh")}
          aria-label={t("refresh")}
          disabled={disabled || state.busy}
          onClick={() => void state.load()}
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
      {state.preview && (
        <GovernanceContent state={state} disabled={disabled || state.busy} />
      )}
    </section>
  )
}

function useGovernance(onUpdated: () => void) {
  const t = useTranslations("UserMemorySettings.governance")
  const [preview, setPreview] = useState<MemoryGovernancePreview | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [acknowledged, setAcknowledged] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const load = () =>
    loadGovernance({
      setPreview,
      setSelected,
      setAcknowledged,
      setBusy,
      setError,
    })

  useEffect(() => {
    void load()
  }, [])

  const apply = () =>
    preview && !busy
      ? applyGovernanceState({
          preview,
          selected,
          acknowledged,
          load,
          onUpdated,
          setBusy,
          setError,
          applied: t("applied"),
        })
      : Promise.resolve()

  return {
    preview,
    selected,
    setSelected,
    acknowledged,
    setAcknowledged,
    busy,
    error,
    load,
    apply,
  }
}

type GovernanceSetters = {
  setPreview: (value: MemoryGovernancePreview | null) => void
  setSelected: (value: string[]) => void
  setAcknowledged: (value: boolean) => void
  setBusy: (value: boolean) => void
  setError: (value: string | null) => void
}

async function loadGovernance(state: GovernanceSetters) {
  state.setBusy(true)
  state.setError(null)
  try {
    const next = await previewMemoryGovernance()
    state.setPreview(next)
    state.setSelected(next.recommendations.map((item) => item.id))
    state.setAcknowledged(false)
  } catch (reason) {
    state.setError(toErrorMessage(reason))
  } finally {
    state.setBusy(false)
  }
}

async function applyGovernanceState(
  input: Pick<GovernanceSetters, "setBusy" | "setError"> & {
    preview: MemoryGovernancePreview
    selected: string[]
    acknowledged: boolean
    load: () => Promise<void>
    onUpdated: () => void
    applied: string
  }
) {
  input.setBusy(true)
  input.setError(null)
  try {
    const result = await applyMemoryGovernance({
      expectedRevision: input.preview.revision,
      ids: input.selected,
      acknowledged: input.acknowledged,
    })
    toast.success(input.applied, {
      description: `${result.stopped} / ${result.recovered}`,
    })
    await input.load()
    input.onUpdated()
  } catch (reason) {
    input.setError(toErrorMessage(reason))
    input.setBusy(false)
  }
}

function GovernanceContent({
  state,
  disabled,
}: {
  state: ReturnType<typeof useGovernance>
  disabled: boolean
}) {
  const t = useTranslations("UserMemorySettings.governance")
  if (state.preview!.recommendations.length === 0) {
    return <p className="text-xs text-muted-foreground">{t("empty")}</p>
  }
  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">
        {t("retained")}: {state.preview!.retainedCount}
      </p>
      <GovernanceRecommendations state={state} disabled={disabled} />
      <GovernanceActions state={state} disabled={disabled} />
    </div>
  )
}

function GovernanceRecommendations({
  state,
  disabled,
}: {
  state: ReturnType<typeof useGovernance>
  disabled: boolean
}) {
  const t = useTranslations("UserMemorySettings.governance")
  const toggle = (id: string, checked: boolean) =>
    state.setSelected(
      checked
        ? [...state.selected, id]
        : state.selected.filter((value) => value !== id)
    )
  return (
    <ul className="divide-y border-y">
      {state.preview!.recommendations.map((item) => (
        <li key={item.id} className="flex items-start gap-2 py-3 text-sm">
          <input
            type="checkbox"
            checked={state.selected.includes(item.id)}
            disabled={disabled}
            onChange={(event) => toggle(item.id, event.target.checked)}
          />
          <span className="min-w-0">
            <span className="block whitespace-pre-wrap break-words">
              {item.content}
            </span>
            <span className="mt-1 block text-xs text-muted-foreground">
              {t(item.reason as "task_scoped_instruction")}
            </span>
          </span>
        </li>
      ))}
    </ul>
  )
}

function GovernanceActions({
  state,
  disabled,
}: {
  state: ReturnType<typeof useGovernance>
  disabled: boolean
}) {
  const t = useTranslations("UserMemorySettings.governance")
  return (
    <>
      <label className="flex items-start gap-2 text-xs">
        <input
          type="checkbox"
          checked={state.acknowledged}
          disabled={disabled || state.selected.length === 0}
          onChange={(event) => state.setAcknowledged(event.target.checked)}
        />
        {t("acknowledge")}
      </label>
      <Button
        size="sm"
        disabled={
          disabled || !state.acknowledged || state.selected.length === 0
        }
        onClick={() => void state.apply()}
      >
        <Check className="size-4" />
        {t("apply")}
      </Button>
    </>
  )
}
