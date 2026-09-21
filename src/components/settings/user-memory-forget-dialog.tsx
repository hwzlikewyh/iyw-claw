"use client"

import { useState } from "react"
import { Loader2, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import type { ForgetUserMemoryResult } from "@/lib/user-memory-entries"

export function UserMemoryForgetDialog({
  disabled,
  onForget,
}: {
  disabled: boolean
  onForget: (purgeBackups: boolean) => Promise<ForgetUserMemoryResult | null>
}) {
  const t = useTranslations("UserMemorySettings.entries")
  const [open, setOpen] = useState(false)
  const [confirmation, setConfirmation] = useState("")
  const [purgeBackups, setPurgeBackups] = useState(false)
  const [busy, setBusy] = useState(false)
  const forget = async () => {
    setBusy(true)
    const result = await onForget(purgeBackups)
    setBusy(false)
    if (!result) return
    setOpen(false)
    toast.success(t("forgotten"), {
      description: result.residualBackupPaths.length
        ? t("backupResidual", { count: result.residualBackupPaths.length })
        : t("noBackupResidual"),
    })
  }
  return (
    <>
      <ForgetTrigger disabled={disabled} open={() => setOpen(true)} />
      <Dialog open={open} onOpenChange={(value) => !busy && setOpen(value)}>
        <DialogContent className="sm:max-w-md">
          <ForgetForm
            busy={busy}
            confirmation={confirmation}
            setConfirmation={setConfirmation}
            purgeBackups={purgeBackups}
            setPurgeBackups={setPurgeBackups}
            forget={forget}
          />
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

function ForgetForm({
  busy,
  confirmation,
  setConfirmation,
  purgeBackups,
  setPurgeBackups,
  forget,
}: {
  busy: boolean
  confirmation: string
  setConfirmation: (value: string) => void
  purgeBackups: boolean
  setPurgeBackups: (value: boolean) => void
  forget: () => Promise<void>
}) {
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
          checked={purgeBackups}
          disabled={busy}
          onChange={(event) => setPurgeBackups(event.target.checked)}
        />
        {t("purgeBackups")}
      </label>
      <Input
        value={confirmation}
        disabled={busy}
        aria-label={t("forgetConfirmation")}
        placeholder="FORGET"
        onChange={(event) => setConfirmation(event.target.value)}
      />
      <Button
        variant="destructive"
        disabled={busy || confirmation !== "FORGET"}
        onClick={() => void forget()}
      >
        {busy ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Trash2 className="size-4" />
        )}
        {t("forget")}
      </Button>
    </>
  )
}
