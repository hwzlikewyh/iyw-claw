"use client"

import { Loader2, RefreshCw, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { ClearUserMemoryScope } from "@/lib/user-memory-entries"
import { useClearUserMemory } from "./use-clear-user-memory"

type ClearState = ReturnType<typeof useClearUserMemory>

export function UserMemoryClearDialog({
  disabled,
  onUpdated,
}: {
  disabled: boolean
  onUpdated: () => void
}) {
  const t = useTranslations("UserMemorySettings.clear")
  const state = useClearUserMemory(onUpdated)
  return (
    <>
      <Button
        size="sm"
        variant="ghost"
        className="text-destructive"
        disabled={disabled}
        onClick={() => state.changeOpen(true)}
      >
        <Trash2 className="size-4" />
        {t("trigger")}
      </Button>
      <Dialog open={state.open} onOpenChange={state.changeOpen}>
        <DialogContent className="sm:max-w-md" showCloseButton={!state.busy}>
          <DialogHeader>
            <DialogTitle>{t("title")}</DialogTitle>
            <DialogDescription>{t("description")}</DialogDescription>
          </DialogHeader>
          <ClearScope state={state} />
          <ClearOptions state={state} />
          <ClearStatus state={state} />
          <ClearFooter state={state} />
        </DialogContent>
      </Dialog>
    </>
  )
}

function ClearScope({ state }: { state: ClearState }) {
  const t = useTranslations("UserMemorySettings.clear")
  return (
    <div className="space-y-2">
      <label className="grid gap-2 text-sm">
        {t("scope")}
        <select
          className="h-9 w-full min-w-0 rounded-md border border-input bg-background px-3 text-sm"
          value={state.scope}
          disabled={state.busy}
          onChange={(event) =>
            state.changeScope(event.target.value as ClearUserMemoryScope)
          }
        >
          <option value="memory">{t("memory")}</option>
          <option value="all">{t("all")}</option>
        </select>
      </label>
      <p className="text-sm text-muted-foreground">
        {t(state.scope === "memory" ? "memoryDescription" : "allDescription")}
      </p>
    </div>
  )
}

function ClearOptions({ state }: { state: ClearState }) {
  const t = useTranslations("UserMemorySettings.clear")
  return (
    <div className="space-y-3 text-sm">
      <label className="flex items-start gap-2">
        <input
          type="checkbox"
          className="mt-1 shrink-0"
          checked={state.purgeBackups}
          disabled={state.busy}
          onChange={(event) => state.setPurgeBackups(event.target.checked)}
        />
        <span>{t("purgeBackups")}</span>
      </label>
      <label className="flex items-start gap-2">
        <input
          type="checkbox"
          className="mt-1 shrink-0"
          checked={state.confirmed}
          disabled={state.busy || !state.ready}
          onChange={(event) => state.setConfirmed(event.target.checked)}
        />
        <span>{t("confirm")}</span>
      </label>
    </div>
  )
}

function ClearStatus({ state }: { state: ClearState }) {
  const t = useTranslations("UserMemorySettings.clear")
  if (state.loading)
    return (
      <p
        role="status"
        className="flex items-center gap-2 text-sm text-muted-foreground"
      >
        <Loader2 className="size-4 animate-spin" />
        {t("loading")}
      </p>
    )
  if (!state.error) return null
  return (
    <div className="flex items-start gap-2">
      <p
        role="alert"
        className="min-w-0 flex-1 break-words text-sm text-destructive"
      >
        {state.error}
      </p>
      <Button
        size="icon"
        variant="ghost"
        title={t("refresh")}
        aria-label={t("refresh")}
        disabled={state.busy}
        onClick={state.refresh}
      >
        <RefreshCw className="size-4" />
      </Button>
    </div>
  )
}

function ClearFooter({ state }: { state: ClearState }) {
  const t = useTranslations("UserMemorySettings.clear")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        disabled={state.busy}
        onClick={() => state.changeOpen(false)}
        autoFocus
      >
        {t("cancel")}
      </Button>
      <Button
        variant="destructive"
        disabled={state.busy || !state.ready || !state.confirmed}
        onClick={() => void state.submit()}
      >
        {state.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Trash2 className="size-4" />
        )}
        {t(state.busy ? "clearing" : "submit")}
      </Button>
    </DialogFooter>
  )
}
