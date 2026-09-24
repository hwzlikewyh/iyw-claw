"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { openSettingsWindow } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

export function useSidebarAccount() {
  const account = useIywAccount()
  const t = useTranslations("SidebarDesign")
  const [open, setOpen] = useState(false)
  const [dialog, setDialog] = useState<
    "login" | "profile" | "logout" | "manager" | null
  >(null)
  const [refreshing, setRefreshing] = useState(false)
  const busy = useRef(false)
  const showDialog = (next: typeof dialog) => {
    setOpen(false)
    setDialog(next)
  }
  const refresh = async () => {
    if (busy.current) return
    busy.current = true
    setRefreshing(true)
    try {
      await account.refreshProfile()
    } catch (error) {
      toast.error(t("refreshFailed", { message: toErrorMessage(error) }))
    } finally {
      busy.current = false
      setRefreshing(false)
    }
  }
  const logout = async () => {
    if (busy.current) return
    busy.current = true
    try {
      await account.logout()
      setDialog(null)
      console.info("[sidebar-account] signed out")
    } catch (error) {
      console.error("[sidebar-account] sign out failed", {
        message: toErrorMessage(error),
      })
      toast.error(t("logoutFailed", { message: toErrorMessage(error) }))
    } finally {
      busy.current = false
    }
  }
  const settings = async () => {
    setOpen(false)
    try {
      await openSettingsWindow("appearance")
    } catch (error) {
      toast.error(t("actionFailed", { message: toErrorMessage(error) }))
    }
  }
  return {
    ...account,
    open,
    setOpen,
    dialog,
    showDialog,
    refreshing,
    refresh,
    signOut: logout,
    settings,
  }
}
