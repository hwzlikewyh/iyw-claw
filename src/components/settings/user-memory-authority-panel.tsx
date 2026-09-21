"use client"

import { useEffect, useState } from "react"
import { Database, Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { toErrorMessage } from "@/lib/app-error"
import {
  activateMemoryAuthority,
  getMemoryAuthority,
  prepareMemoryAuthority,
  type MemoryAuthorityStatus,
} from "@/lib/user-memory-authority"

function useAuthority(onUpdated: () => void) {
  const [status, setStatus] = useState<MemoryAuthorityStatus | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let current = true
    getMemoryAuthority()
      .then((value) => current && setStatus(value))
      .catch((reason) => current && setError(toErrorMessage(reason)))
    return () => {
      current = false
    }
  }, [])
  const run = async (action: () => Promise<MemoryAuthorityStatus>) => {
    if (busy) return false
    setBusy(true)
    setError(null)
    try {
      setStatus(await action())
      if (action !== getMemoryAuthority) onUpdated()
      return true
    } catch (reason) {
      setError(toErrorMessage(reason))
      return false
    } finally {
      setBusy(false)
    }
  }
  return { status, busy, error, run }
}

type AuthorityState = ReturnType<typeof useAuthority>

export function UserMemoryAuthorityPanel({
  onUpdated,
  disabled = false,
}: {
  onUpdated: () => void
  disabled?: boolean
}) {
  const t = useTranslations("UserMemorySettings.authority")
  const state = useAuthority(onUpdated)
  const [confirm, setConfirm] = useState(false)
  return (
    <section className="space-y-3 border-b py-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="flex items-center gap-2 text-sm font-medium">
          <Database className="size-4" />
          {t("title")}
        </h2>
        <AuthorityActions
          state={state}
          disabled={disabled}
          onActivate={() => setConfirm(true)}
        />
      </div>
      {state.error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {state.error}
        </p>
      )}
      {state.status ? (
        <AuthorityDetails status={state.status} />
      ) : (
        !state.error && (
          <p role="status" className="text-xs text-muted-foreground">
            {t("loading")}
          </p>
        )
      )}
      <ActivationDialog
        state={state}
        open={confirm}
        onOpenChange={setConfirm}
      />
    </section>
  )
}

function AuthorityActions({
  state,
  disabled,
  onActivate,
}: {
  state: AuthorityState
  disabled: boolean
  onActivate: () => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  return (
    <div className="flex flex-wrap items-center gap-2">
      {state.status?.mode !== "active" && (
        <Button
          size="sm"
          variant="outline"
          disabled={disabled || state.busy || !state.status}
          onClick={() => void state.run(prepareMemoryAuthority)}
        >
          {t("prepare")}
        </Button>
      )}
      {state.status?.mode === "shadow" && (
        <Button
          size="sm"
          disabled={disabled || state.busy}
          onClick={onActivate}
        >
          {t("activate")}
        </Button>
      )}
      <Button
        size="icon"
        variant="ghost"
        aria-label={t("refresh")}
        title={t("refresh")}
        disabled={state.busy}
        onClick={() => void state.run(getMemoryAuthority)}
      >
        {state.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <RefreshCw className="size-4" />
        )}
      </Button>
    </div>
  )
}

function ActivationDialog({
  state,
  open,
  onOpenChange,
}: {
  state: AuthorityState
  open: boolean
  onOpenChange: (value: boolean) => void
}) {
  const t = useTranslations("UserMemorySettings.authority")
  const activate = async () => {
    const revision = state.status?.revision
    if (revision && (await state.run(() => activateMemoryAuthority(revision))))
      onOpenChange(false)
  }
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => !state.busy && onOpenChange(value)}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("activate")}</DialogTitle>
          <DialogDescription>{t("activateWarning")}</DialogDescription>
        </DialogHeader>
        <Button
          disabled={state.busy || !state.status?.revision}
          onClick={() => void activate()}
        >
          {state.busy && <Loader2 className="size-4 animate-spin" />}
          {t("activate")}
        </Button>
        {state.error && (
          <p role="alert" className="break-words text-xs text-destructive">
            {state.error}
          </p>
        )}
      </DialogContent>
    </Dialog>
  )
}

function AuthorityDetails({ status }: { status: MemoryAuthorityStatus }) {
  const t = useTranslations("UserMemorySettings.authority")
  return (
    <div className="space-y-2 text-xs text-muted-foreground">
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        <span>{t(status.mode)}</span>
        <span>
          {t("records")}: {status.records}
        </span>
        <span>
          {t("revisions")}: {status.revisions}
        </span>
        <span>
          {t("pending")}: {status.pendingProjections}
        </span>
      </div>
      <div className="flex flex-wrap gap-3">
        {Object.entries(status.counts).map(([kind, count]) => (
          <span key={kind}>
            {kind}: {count}
          </span>
        ))}
      </div>
      {status.backupPath && (
        <p className="break-all">
          {t("backup")}: {status.backupPath}
        </p>
      )}
      {status.externalChanges.length > 0 && (
        <p
          role="alert"
          className="break-words text-amber-700 dark:text-amber-400"
        >
          {t("externalConflict")}: {status.externalChanges.join(", ")}
        </p>
      )}
    </div>
  )
}
