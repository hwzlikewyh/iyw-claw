"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { ArrowUpCircle, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import {
  type UpdateContextValue,
  useAppUpdate,
} from "@/components/providers/update-provider"
import { Button } from "@/components/ui/button"
import { Dialog, DialogTrigger } from "@/components/ui/dialog"
import {
  ACTIVE_UPDATE_STATUSES,
  hasFreshCheck,
  type UpdateDetails,
} from "@/components/layout/title-bar-update-model"
import { TitleBarUpdateDialog } from "@/components/layout/title-bar-update-dialog"
import { useUpdateOfferToast } from "@/components/layout/title-bar-update-toast"
import { extractAppCommandError } from "@/lib/app-error"
import {
  type AppUpdateCheckResult,
  type AppUpdateState,
  checkAppUpdate,
  getCurrentAppVersion,
  getServerUpdateStatus,
  normalizeAppUpdateError,
  usesTauriUpdater,
} from "@/lib/updater"
import { cn } from "@/lib/utils"

const BADGE_STATUSES = new Set<AppUpdateState["status"]>([
  "available",
  "downloading",
  "verifying",
  "installing",
  "ready_to_restart",
])

function isMissingUpdateStatusRoute(error: unknown): boolean {
  const outer = extractAppCommandError(error)
  if (outer?.code === "not_implemented") return true
  if (!outer?.detail) return false
  return (
    extractAppCommandError(new Error(outer.detail))?.code === "not_implemented"
  )
}

async function loadUpdateDetails(): Promise<UpdateDetails> {
  if (usesTauriUpdater()) {
    return {
      currentVersion: await getCurrentAppVersion(),
      canInstall: true,
      checked: false,
      checkedSeq: null,
      liveProgress: true,
      loaded: true,
      availableUpdate: null,
    }
  }
  let liveProgress: boolean | null = null
  try {
    const status = await getServerUpdateStatus()
    if (status) {
      return {
        currentVersion: status.currentVersion,
        canInstall: status.selfUpdateSupported && Boolean(status.liveProgress),
        checked: false,
        checkedSeq: null,
        liveProgress: Boolean(status.liveProgress),
        loaded: true,
        runtime: status.runtime,
        availableUpdate: null,
      }
    }
  } catch (error) {
    if (isMissingUpdateStatusRoute(error)) liveProgress = false
    console.warn("[TitleBarUpdate] failed to load server capability:", error)
  }
  return {
    currentVersion: await getCurrentAppVersion(),
    canInstall: false,
    checked: false,
    checkedSeq: null,
    liveProgress,
    loaded: true,
    availableUpdate: null,
  }
}

function detailsFromCheck(
  result: AppUpdateCheckResult,
  checkedSeq: number
): UpdateDetails {
  return {
    currentVersion: result.currentVersion,
    canInstall:
      usesTauriUpdater() ||
      Boolean(result.selfUpdateSupported && result.liveProgress),
    checked: true,
    checkedSeq,
    liveProgress: usesTauriUpdater() || Boolean(result.liveProgress),
    loaded: true,
    runtime: result.runtime,
    availableUpdate: result.update,
  }
}

function useUpdateDetails(open: boolean) {
  const checkAppliedRef = useRef(false)
  const [details, setDetails] = useState<UpdateDetails>({
    currentVersion: "",
    canInstall: usesTauriUpdater(),
    checked: false,
    checkedSeq: null,
    liveProgress: usesTauriUpdater(),
    loaded: false,
    availableUpdate: null,
  })

  useEffect(() => {
    if (!open) return
    let cancelled = false
    checkAppliedRef.current = false
    void loadUpdateDetails().then((next) => {
      if (!cancelled && !checkAppliedRef.current) setDetails(next)
    })
    return () => {
      cancelled = true
    }
  }, [open])

  const applyCheckDetails = useCallback((next: UpdateDetails) => {
    checkAppliedRef.current = true
    setDetails(next)
  }, [])

  return { details, applyCheckDetails }
}

function useManualUpdateCheck(
  update: UpdateContextValue,
  setDetails: (details: UpdateDetails) => void
) {
  const checkingRef = useRef(false)
  const [checking, setChecking] = useState(false)
  const [checkErrorKind, setCheckErrorKind] = useState<
    ReturnType<typeof normalizeAppUpdateError>["kind"] | null
  >(null)

  const runCheck = useCallback(async () => {
    if (checkingRef.current || update.isBusy) return
    checkingRef.current = true
    setChecking(true)
    setCheckErrorKind(null)
    try {
      const result = await checkAppUpdate()
      setDetails(detailsFromCheck(result, update.state.seq))
    } catch (error) {
      const { kind } = normalizeAppUpdateError(error)
      setCheckErrorKind(kind)
      console.warn("[TitleBarUpdate] check failed:", error)
    } finally {
      checkingRef.current = false
      setChecking(false)
    }
  }, [setDetails, update.isBusy, update.state.seq])

  return { checking, checkErrorKind, runCheck }
}

function useCheckOnOpen(
  open: boolean,
  update: UpdateContextValue,
  details: UpdateDetails,
  runCheck: () => Promise<void>
) {
  const attemptedRef = useRef(false)
  useEffect(() => {
    if (!open) {
      attemptedRef.current = false
      return
    }
    const legacyServerReady =
      !usesTauriUpdater() && details.loaded && !details.liveProgress
    const ready = update.hydrated || legacyServerReady
    if (!ready || attemptedRef.current) return
    attemptedRef.current = true
    if (update.state.status === "idle" || update.state.status === "error") {
      void runCheck()
    }
  }, [details, open, runCheck, update.hydrated, update.state.status])
}

interface ControlProps {
  update: UpdateContextValue
  mobile: boolean
}

function isControlActive(update: UpdateContextValue, checking: boolean) {
  return (
    update.isRestarting ||
    checking ||
    ACTIVE_UPDATE_STATUSES.has(update.state.status)
  )
}

function UpdateTriggerIcon({
  active,
  mobile,
}: {
  active: boolean
  mobile: boolean
}) {
  const Icon = active ? Loader2 : ArrowUpCircle
  return (
    <Icon
      aria-hidden="true"
      className={cn(active && "animate-spin", mobile ? "size-4" : "size-3.5")}
    />
  )
}

function TitleBarUpdateControlInner({ update, mobile }: ControlProps) {
  const t = useTranslations("SystemSettings")
  const [open, setOpen] = useState(false)
  const { details, applyCheckDetails } = useUpdateDetails(open)
  const manualCheck = useManualUpdateCheck(update, applyCheckDetails)
  useCheckOnOpen(open, update, details, manualCheck.runCheck)
  const active = isControlActive(update, manualCheck.checking)
  const checkedUpdateAvailable =
    hasFreshCheck(update.state, details) &&
    !manualCheck.checkErrorKind &&
    Boolean(details.availableUpdate)
  useUpdateOfferToast({
    state: update.state,
    details,
    checkedUpdateAvailable,
    setOpen,
  })
  const showBadge =
    BADGE_STATUSES.has(update.state.status) || checkedUpdateAvailable
  const dialog = { details, ...manualCheck }
  const targetVersion = showBadge
    ? (update.state.version ?? details.availableUpdate?.version)
    : null
  const label = targetVersion
    ? t("foundUpdate", { version: targetVersion })
    : t("versionTitle")

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className={cn(
            "relative shrink-0 hover:text-foreground/80",
            mobile ? "size-8" : "size-6"
          )}
          title={label}
          aria-label={label}
          aria-busy={active || undefined}
        >
          <UpdateTriggerIcon active={active} mobile={mobile} />
          {showBadge ? (
            <span
              aria-hidden="true"
              className="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-destructive ring-1 ring-background"
            />
          ) : null}
        </Button>
      </DialogTrigger>
      <TitleBarUpdateDialog update={update} dialog={dialog} />
    </Dialog>
  )
}

export function TitleBarUpdateControl({
  mobile = false,
}: {
  mobile?: boolean
}) {
  const update = useAppUpdate()
  if (!update) return null
  return <TitleBarUpdateControlInner update={update} mobile={mobile} />
}
