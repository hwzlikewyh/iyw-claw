"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"

import { useIsMac } from "@/hooks/use-is-mac"

import { toErrorMessage } from "@/lib/app-error"
import {
  computerShareApp,
  computerShareScreen,
  computerShareWindow,
  computerSharedState,
  computerStop,
} from "@/lib/computer/computer-api"
import {
  clearComputerActivity,
  computerStoreMark,
  setComputerSharedSince,
  setComputerStateSince,
  useComputerStore,
  type ComputerActivityLine,
} from "@/lib/computer/computer-store"
import { stopShortcutLabel } from "@/lib/computer/stop-shortcut"
import type { GrantLevel } from "@/lib/computer/types"
import {
  hostHoldsPermission,
  useComputerStatus,
} from "@/lib/computer/use-computer-status"
import { useComputerStopKey } from "@/lib/computer/use-stop-key"
import {
  useComputerHostState,
  useComputerTransport,
} from "@/lib/computer/use-computer-host"
import { getTransport } from "@/lib/transport"

import { SCREEN_TARGET_ID } from "../status-bar-computer"

export function useComputerPopoverState() {
  const t = useTranslations("ComputerUse")
  const transport = useComputerTransport()
  const { shared, sharedApps, sharedScreen, backend, activity } =
    useComputerStore()
  const ownWindows = shared.filter((w) => !w.wholeApp && !w.wholeScreen)
  const rows = (sharedScreen ? 1 : 0) + sharedApps.length + ownWindows.length
  const anyShared = rows > 0
  const isMac = useIsMac()
  const stopKey = useComputerStopKey()
  const [open, setOpen] = useComputerHostState(false)
  const [pickerOpen, setPickerOpen] = useComputerHostState(false)
  const [stopping, setStopping] = useComputerHostState(false)
  const {
    status,
    loading,
    error,
    setError,
    refresh,
    request,
    requesting,
    revealHelper,
  } = useComputerStatus(open)
  const aliveRef = useRef(true)
  const contentRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    aliveRef.current = true
    return () => {
      aliveRef.current = false
    }
  }, [])
  useEffect(() => {
    const mark = computerStoreMark()
    computerSharedState()
      .then((s) => setComputerStateSince(s, mark))
      .catch(() => {})
  }, [transport])
  const handleOpenChange = (next: boolean) => {
    setOpen(next)
  }
  const setLevel = async (targetId: string, level: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    try {
      setComputerSharedSince(await computerShareWindow(targetId, level), mark)
    } catch (e) {
      setError(toErrorMessage(e))
    }
  }
  const setAppLevel = async (appId: string, level: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    try {
      setComputerStateSince(await computerShareApp({ appId }, level), mark)
    } catch (e) {
      setError(toErrorMessage(e))
    }
  }
  const setScreenLevel = async (level: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    try {
      setComputerStateSince(await computerShareScreen(level), mark)
    } catch (e) {
      setError(toErrorMessage(e))
    }
  }
  const stopSharing = async () => {
    if (transport !== getTransport()) return
    setStopping(true)
    try {
      await computerStop()
    } catch (e) {
      setError(toErrorMessage(e))
    } finally {
      if (aliveRef.current) setStopping(false)
    }
  }
  const clearActivity = () => {
    contentRef.current?.focus()
    clearComputerActivity()
  }
  const liveBackend = backend ?? status?.backend ?? null
  const hostLeaks = hostHoldsPermission(status)
  const permissions = status?.permissions
  const development = liveBackend?.peer === "development"
  const shortcut = stopKey?.active
    ? stopShortcutLabel(stopKey.active, isMac)
    : null
  const appNameOf = (line: ComputerActivityLine) =>
    line.app ??
    (line.targetId === SCREEN_TARGET_ID
      ? t("shared.screen")
      : (shared.find((w) => w.targetId === line.targetId)?.appName ??
        line.targetId))
  return {
    t,
    shared,
    sharedApps,
    sharedScreen,
    activity,
    ownWindows,
    rows,
    anyShared,
    open,
    setOpen,
    pickerOpen,
    setPickerOpen,
    stopping,
    status,
    loading,
    error,
    refresh,
    request,
    requesting,
    revealHelper,
    contentRef,
    handleOpenChange,
    setLevel,
    setAppLevel,
    setScreenLevel,
    stopSharing,
    clearActivity,
    liveBackend,
    hostLeaks,
    permissions,
    development,
    shortcut,
    appNameOf,
  }
}
