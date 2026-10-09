"use client"

import { useEffect } from "react"
import { toast } from "sonner"
import { useTranslations } from "next-intl"
import { toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"
import { computerStop, useComputerAvailable } from "./computer-api"
import { useComputerStore } from "./computer-store"
import { useComputerEnabled } from "./use-computer-enabled"
import { useComputerHostState, useComputerTransport } from "./use-computer-host"

export function useComputerSharing() {
  const t = useTranslations("ComputerUse")
  const available = useComputerAvailable()
  const transport = useComputerTransport()
  const { enabled } = useComputerEnabled({ desktopOnly: false })
  const [open, setOpen] = useComputerHostState(false)
  const [pending, setPending] = useComputerHostState(false)
  const { shared, sharedApps, sharedScreen } = useComputerStore()
  useEffect(() => {
    if (open && (!available || enabled !== true)) setOpen(false)
  }, [open, available, enabled, setOpen])
  const count =
    shared.filter((window) => !window.wholeApp && !window.wholeScreen).length +
    sharedApps.length +
    Number(!!sharedScreen)
  const begin = () => {
    if (pending || !available || !enabled || transport !== getTransport())
      return
    setOpen(true)
  }
  const stop = async () => {
    if (pending || transport !== getTransport()) return
    setPending(true)
    try {
      await computerStop()
    } catch (error) {
      if (transport === getTransport()) toast.error(toErrorMessage(error))
    } finally {
      setPending(false)
    }
  }
  const label = count ? t("manageShared", { count }) : t("share")
  return {
    available,
    enabled,
    pending,
    open,
    setOpen,
    count,
    begin,
    stop,
    label,
  }
}

export type ComputerSharingControl = ReturnType<typeof useComputerSharing>
