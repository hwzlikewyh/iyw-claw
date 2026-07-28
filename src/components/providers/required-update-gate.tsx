"use client"

import { useEffect, useState } from "react"
import { ArrowUpCircle, Loader2, ShieldAlert } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import type { AppUpdateState } from "@/lib/updater"

interface RequiredUpdateGateProps {
  state: AppUpdateState
  onStart: () => Promise<void>
  onRestart: () => Promise<void>
}

function useDeadlineClock(state: AppUpdateState) {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    if (state.updatePolicy !== "required" || !state.enforceAfter) return
    const deadline = Date.parse(state.enforceAfter)
    if (!Number.isFinite(deadline) || deadline <= now) return
    const timer = window.setTimeout(
      () => setNow(Date.now()),
      Math.min(deadline - now + 50, 60_000)
    )
    return () => window.clearTimeout(timer)
  }, [now, state.enforceAfter, state.updatePolicy])

  return now
}

function isEnforced(state: AppUpdateState, now: number) {
  if (state.updatePolicy !== "required" || !state.version) return false
  if (!state.enforceAfter) return true
  const deadline = Date.parse(state.enforceAfter)
  return !Number.isFinite(deadline) || deadline <= now
}

function GateDescription({
  state,
  deadline,
}: {
  state: AppUpdateState
  deadline: string | null
}) {
  const t = useTranslations("SystemSettings")
  if (state.status === "error" && state.error) {
    return t("installFailed", { message: state.error })
  }
  return deadline
    ? t("requiredUpdateDeadline", { time: deadline })
    : t("requiredUpdateHint")
}

function GateAction({ state, onStart, onRestart }: RequiredUpdateGateProps) {
  const t = useTranslations("SystemSettings")
  const installing = ["downloading", "verifying", "installing"].includes(
    state.status
  )
  const checking = state.status === "checking"
  const restarting = state.status === "restarting"
  if (state.status === "ready_to_restart") {
    return (
      <Button onClick={() => void onRestart()}>
        <ArrowUpCircle />
        {t("restartToUpdate")}
      </Button>
    )
  }
  if (checking || installing || restarting) {
    return (
      <Button disabled>
        <Loader2 className="animate-spin" />
        {checking
          ? t("checking")
          : installing
            ? t("updating")
            : t("restarting")}
      </Button>
    )
  }
  return (
    <Button onClick={() => void onStart()}>
      <ArrowUpCircle />
      {t("upgradeTo", { version: state.version ?? "" })}
    </Button>
  )
}

export function RequiredUpdateGate({
  state,
  onStart,
  onRestart,
}: RequiredUpdateGateProps) {
  const t = useTranslations("SystemSettings")
  const locale = useLocale()
  const now = useDeadlineClock(state)
  if (!isEnforced(state, now)) return null

  const busy = [
    "downloading",
    "verifying",
    "installing",
    "restarting",
  ].includes(state.status)
  const deadlineMs = state.enforceAfter
    ? Date.parse(state.enforceAfter)
    : Number.NaN
  const deadline = Number.isFinite(deadlineMs)
    ? new Date(deadlineMs).toLocaleString(locale)
    : null

  return (
    <AlertDialog open onOpenChange={() => {}}>
      <AlertDialogContent onEscapeKeyDown={(event) => event.preventDefault()}>
        <AlertDialogHeader>
          <AlertDialogMedia>
            {busy ? (
              <Loader2 className="animate-spin" />
            ) : (
              <ShieldAlert className="text-destructive" />
            )}
          </AlertDialogMedia>
          <AlertDialogTitle>{t("requiredUpdate")}</AlertDialogTitle>
          <AlertDialogDescription>
            <GateDescription state={state} deadline={deadline} />
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <GateAction state={state} onStart={onStart} onRestart={onRestart} />
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
