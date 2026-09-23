"use client"

import { useRef, useState } from "react"
import { Loader2, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { toErrorMessage } from "@/lib/app-error"
import type { ForgetUserMemoryResult } from "@/lib/user-memory-entries"

type ForgetAction = (
  purgeBackups: boolean
) => Promise<ForgetUserMemoryResult | null>

function useForgetDialog(onForget: ForgetAction) {
  const t = useTranslations("UserMemorySettings.entries")
  const pending = useRef(false)
  const [open, setOpen] = useState(false)
  const [purgeBackups, setPurgeBackups] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const changeOpen = (value: boolean) => {
    if (pending.current) return
    setError(null)
    setPurgeBackups(false)
    setOpen(value)
  }
  const forget = async () => {
    if (pending.current) return
    pending.current = true
    setBusy(true)
    setError(null)
    try {
      const result = await onForget(purgeBackups)
      if (!result?.forgotten) return setError(t("forgetFailed"))
      setOpen(false)
      toast.success(t("forgotten"), {
        description: result.residualBackupPaths.length
          ? t("backupResidual", { count: result.residualBackupPaths.length })
          : t("noBackupResidual"),
      })
    } catch (reason) {
      setError(toErrorMessage(reason))
    } finally {
      pending.current = false
      setBusy(false)
    }
  }
  return {
    open,
    changeOpen,
    purgeBackups,
    setPurgeBackups,
    busy,
    error,
    forget,
  }
}

export function UserMemoryForgetDialog(props: {
  disabled: boolean
  onForget: (purgeBackups: boolean) => Promise<ForgetUserMemoryResult | null>
}) {
  const state = useForgetDialog(props.onForget)
  return (
    <>
      <ForgetTrigger
        disabled={props.disabled}
        open={() => state.changeOpen(true)}
      />
      <Dialog open={state.open} onOpenChange={state.changeOpen}>
        <DialogContent className="sm:max-w-md" showCloseButton={!state.busy}>
          <ForgetForm state={state} />
        </DialogContent>
      </Dialog>
    </>
  )
}

function ForgetTrigger({
  disabled,
  open,
}: {
  disabled: boolean
  open: () => void
}) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <Button
      variant="ghost"
      size="icon"
      title={t("forget")}
      aria-label={t("forget")}
      disabled={disabled}
      onClick={open}
    >
      <Trash2 className="size-4" />
    </Button>
  )
}

function ForgetForm({ state }: { state: ReturnType<typeof useForgetDialog> }) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("forgetTitle")}</DialogTitle>
        <DialogDescription>{t("forgetDescription")}</DialogDescription>
      </DialogHeader>
      <label className="flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          checked={state.purgeBackups}
          disabled={state.busy}
          onChange={(event) => state.setPurgeBackups(event.target.checked)}
        />
        {t("purgeBackups")}
      </label>
      {state.error && (
        <p role="alert" className="break-words text-sm text-destructive">
          {state.error}
        </p>
      )}
      <ForgetFooter state={state} />
    </>
  )
}

function ForgetFooter({
  state,
}: {
  state: ReturnType<typeof useForgetDialog>
}) {
  const t = useTranslations("UserMemorySettings.entries")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        disabled={state.busy}
        onClick={() => state.changeOpen(false)}
        autoFocus
      >
        {t("forgetCancel")}
      </Button>
      <Button
        variant="destructive"
        disabled={state.busy}
        onClick={() => void state.forget()}
      >
        {state.busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Trash2 className="size-4" />
        )}
        {t("forget")}
      </Button>
    </DialogFooter>
  )
}
