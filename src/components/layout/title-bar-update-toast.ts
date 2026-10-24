"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import type { UpdateDetails } from "@/components/layout/title-bar-update-model"
import type { AppUpdateState } from "@/lib/updater"

const notifiedUpdateVersions = new Set<string>()
let openCurrentUpdateDialog: (() => void) | null = null

interface UpdateOfferToastOptions {
  state: AppUpdateState
  details: UpdateDetails
  checkedUpdateAvailable: boolean
  setOpen: (open: boolean) => void
}

export function useUpdateOfferToast({
  state,
  details,
  checkedUpdateAvailable,
  setOpen,
}: UpdateOfferToastOptions) {
  const t = useTranslations("SystemSettings")
  const downloaded = state.status === "ready_to_install"
  const version =
    state.status === "available" || downloaded
      ? state.version
      : checkedUpdateAvailable
        ? details.availableUpdate?.version
        : null

  useEffect(() => {
    const open = () => setOpen(true)
    openCurrentUpdateDialog = open
    return () => {
      if (openCurrentUpdateDialog === open) openCurrentUpdateDialog = null
    }
  }, [setOpen])

  useEffect(() => {
    if (!version) return
    const notificationKey = `${version}:${downloaded ? "ready" : "available"}`
    if (notifiedUpdateVersions.has(notificationKey)) return
    notifiedUpdateVersions.add(notificationKey)
    const notify = downloaded ? toast.success : toast.info
    notify(
      downloaded ? t("updateDownloadedHint") : t("foundUpdate", { version }),
      {
        id: `app-update-${version}`,
        action: {
          label: t("viewUpdate"),
          onClick: () => openCurrentUpdateDialog?.(),
        },
      }
    )
  }, [downloaded, setOpen, t, version])
}
