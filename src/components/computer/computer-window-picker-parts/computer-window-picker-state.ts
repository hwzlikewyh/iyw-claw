"use client"

import { useCallback, useEffect, useRef } from "react"
import { useTranslations } from "next-intl"

import { toErrorMessage } from "@/lib/app-error"
import {
  computerAvailable,
  computerListShareableWindows,
  computerRevokeAll,
  computerShareApp,
  computerShareScreen,
  computerShareWindow,
  computerShareWindows,
} from "@/lib/computer/computer-api"
import {
  computerStoreMark,
  setComputerSharedSince,
  setComputerStateSince,
  useComputerStore,
} from "@/lib/computer/computer-store"
import type {
  GrantLevel,
  NotGrantable,
  PickerWindow,
} from "@/lib/computer/types"
import { useComputerStatus } from "@/lib/computer/use-computer-status"
import {
  useComputerHostState,
  useComputerTransport,
} from "@/lib/computer/use-computer-host"
import { getTransport } from "@/lib/transport"

import { AppGroup } from "../computer-window-picker"
import { groupWindows, windowShareState } from "./window-groups"

export function useComputerWindowPickerState({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations("ComputerUse.picker")
  const tComputer = useTranslations("ComputerUse")
  const transport = useComputerTransport()
  const { shared, sharedScreen, sharedKnown } = useComputerStore()
  const stateOf = (window: PickerWindow) =>
    windowShareState(window, { known: sharedKnown, windows: shared })
  const levelOf = (w: PickerWindow): GrantLevel => stateOf(w).level
  const [windows, setWindows] = useComputerHostState<PickerWindow[] | null>(
    null
  )
  const [error, setError] = useComputerHostState<string | null>(null)
  const [notice, setNotice] = useComputerHostState<string | null>(null)
  const [busy, setBusy] = useComputerHostState<{
    targetId: string
    level: GrantLevel
  } | null>(null)
  const [busyApp, setBusyApp] = useComputerHostState<{
    key: string
    level: GrantLevel
  } | null>(null)
  const [busyScreen, setBusyScreen] = useComputerHostState<GrantLevel | null>(
    null
  )
  const [bulk, setBulk] = useComputerHostState(false)
  const changing =
    bulk || busy !== null || busyApp !== null || busyScreen !== null
  const [showUnshareable, setShowUnshareable] = useComputerHostState(false)
  const [pictures, setPictures] = useComputerHostState(0)
  const loadSeqRef = useRef(0)
  const {
    status,
    error: permissionError,
    request,
    requesting,
  } = useComputerStatus(open && computerAvailable())
  const permissions = status?.permissions
  const screenRecording = permissions?.required
    ? permissions.screenRecording
    : undefined
  const screenLevel: GrantLevel = sharedScreen?.level ?? "none"
  const screenShared = screenLevel !== "none"
  const screenOffered = !!status?.screenOffered || screenShared
  const load = useCallback(async () => {
    if (transport !== getTransport()) return
    const seq = ++loadSeqRef.current
    setError(null)
    try {
      const listed = await computerListShareableWindows()
      if (seq === loadSeqRef.current) setWindows(listed)
    } catch (e) {
      if (seq !== loadSeqRef.current) return
      setError(toErrorMessage(e))
      setWindows([])
    }
  }, [transport, setWindows, setError])
  useEffect(() => {
    if (open) {
      setWindows(null)
      setNotice(null)
      void load()
    }
  }, [open, load, setWindows, setNotice])
  const reload = useCallback(() => {
    setPictures((n) => n + 1)
    return load()
  }, [load, setPictures])
  const missedRef = useRef(false)
  useEffect(() => {
    if (screenRecording === false) {
      missedRef.current = true
    } else if (screenRecording && missedRef.current) {
      missedRef.current = false
      void reload()
    }
  }, [screenRecording, reload])
  const shareable = windows?.filter((w) => !w.notGrantable) ?? []
  const unshareable =
    windows?.filter(
      (
        w
      ): w is PickerWindow & {
        notGrantable: NotGrantable
      } => !!w.notGrantable
    ) ?? []
  const sharedCount = shareable.filter((w) => levelOf(w) !== "none").length
  const anyShared = sharedCount > 0
  const groups = groupWindows(shareable, t("unnamedApp"))
  const appOf = (
    group: AppGroup
  ): {
    level: GrantLevel
    appId?: string
  } => {
    for (const w of group.windows) {
      const state = stateOf(w)
      if (state.wholeApp) return { level: state.level, appId: state.appId }
    }
    return { level: "none" }
  }
  const shareAll = async (next: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    setBulk(true)
    setError(null)
    setNotice(null)
    try {
      const result = await computerShareWindows(
        shareable.filter((w) => !stateOf(w).wholeApp).map((w) => w.targetId),
        next
      )
      setComputerSharedSince(result.shared, mark)
      if (result.skipped > 0) {
        setNotice(t("skipped", { count: result.skipped }))
        void load()
      }
    } catch (e) {
      setError(toErrorMessage(e))
      void load()
    } finally {
      setBulk(false)
    }
  }
  const stopAll = async () => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    setBulk(true)
    setError(null)
    setNotice(null)
    try {
      await computerRevokeAll()
      setComputerSharedSince([], mark)
    } catch (e) {
      setError(toErrorMessage(e))
    } finally {
      setBulk(false)
    }
  }
  const setAppLevel = async (
    group: AppGroup,
    appId: string | undefined,
    next: GrantLevel
  ) => {
    if (transport !== getTransport()) return
    const first = group.windows[0]
    if (!appId && !first) return
    const mark = computerStoreMark()
    setBusyApp({ key: group.key, level: next })
    setError(null)
    try {
      setComputerStateSince(
        await computerShareApp(
          appId ? { appId } : { targetId: first.targetId },
          next
        ),
        mark
      )
    } catch (e) {
      setError(toErrorMessage(e))
      void load()
    } finally {
      setBusyApp(null)
    }
  }
  const setScreenLevel = async (next: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    setBusyScreen(next)
    setError(null)
    try {
      setComputerStateSince(await computerShareScreen(next), mark)
    } catch (e) {
      setError(toErrorMessage(e))
    } finally {
      setBusyScreen(null)
    }
  }
  const setLevel = async (item: PickerWindow, next: GrantLevel) => {
    if (transport !== getTransport()) return
    const mark = computerStoreMark()
    setBusy({ targetId: item.targetId, level: next })
    setError(null)
    try {
      setComputerSharedSince(
        await computerShareWindow(item.targetId, next),
        mark
      )
    } catch (e) {
      setError(toErrorMessage(e))
      void load()
    } finally {
      setBusy(null)
    }
  }
  return {
    t,
    tComputer,
    stateOf,
    levelOf,
    windows,
    error,
    notice,
    busy,
    busyApp,
    busyScreen,
    bulk,
    changing,
    showUnshareable,
    setShowUnshareable,
    pictures,
    permissionError,
    request,
    requesting,
    screenRecording,
    screenLevel,
    screenShared,
    screenOffered,
    reload,
    shareable,
    unshareable,
    sharedCount,
    anyShared,
    groups,
    appOf,
    shareAll,
    stopAll,
    setAppLevel,
    setScreenLevel,
    setLevel,
    open,
    onOpenChange,
  }
}
