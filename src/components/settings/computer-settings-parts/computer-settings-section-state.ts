"use client"

import { useCallback, useEffect, useRef } from "react"
import { useTranslations } from "next-intl"

import { toast } from "sonner"
import { usePlatform } from "@/hooks/use-platform"

import { toErrorMessage } from "@/lib/app-error"
import {
  computerServerPlatform,
  getComputerToolsSettings,
  setComputerToolsPreferences,
  useComputerAvailable,
} from "@/lib/computer/computer-api"
import {
  COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT,
  type ComputerToolsSettings,
  type DefaultBlock,
} from "@/lib/computer/types"
import { isLocalDesktop, subscribe } from "@/lib/platform"
import { getTransport } from "@/lib/transport"
import {
  useComputerHostState,
  useComputerTransport,
} from "@/lib/computer/use-computer-host"
import { fromSettings } from "./from-settings"
import { ttlDirty } from "./ttl-dirty"
import { blocklistDirty } from "./blocklist-dirty"
import { removedDirty } from "./removed-dirty"
import { stopShortcutDirty } from "./stop-shortcut-dirty"
import { showIndicatorDirty } from "./show-indicator-dirty"
import { allowForegroundDirty } from "./allow-foreground-dirty"
import { launchEnabledDirty } from "./launch-enabled-dirty"
import { clipboardEnabledDirty } from "./clipboard-enabled-dirty"
import { screenEnabledDirty } from "./screen-enabled-dirty"
import { defaultDeliveryDirty } from "./default-delivery-dirty"

import { Values, EMPTY } from "../computer-settings"

export function useComputerSettingsSectionState() {
  const t = useTranslations("ComputerUse.settings")
  const tComputer = useTranslations("ComputerUse")
  const transport = useComputerTransport()
  const [loaded, setLoaded] = useComputerHostState(false)
  const [loading, setLoading] = useComputerHostState(true)
  const [saving, setSaving] = useComputerHostState(false)
  const [loadError, setLoadError] = useComputerHostState<string | null>(null)
  const [values, setValues] = useComputerHostState<Values>(EMPTY)
  const [baseline, setBaseline] = useComputerHostState<Values>(EMPTY)
  const [enabled, setEnabled] = useComputerHostState(false)
  const [defaults, setDefaults] = useComputerHostState<DefaultBlock[]>([])
  const valuesRef = useRef(values)
  const baselineRef = useRef(baseline)
  useEffect(() => {
    valuesRef.current = values
    baselineRef.current = baseline
  }, [values, baseline])
  const remoteGenRef = useRef(0)
  const remoteRef = useRef<Values | null>(null)
  const applyRead = useCallback(
    (settings: ComputerToolsSettings, gen: number) => {
      if (transport !== getTransport()) return
      if (remoteGenRef.current === gen) {
        setValues(fromSettings(settings))
        setBaseline(fromSettings(settings))
        setEnabled(settings.enabled)
      }
      setDefaults(settings.blocklistDefaults)
      setLoaded(true)
      setLoadError(null)
    },
    [
      transport,
      setValues,
      setBaseline,
      setEnabled,
      setDefaults,
      setLoaded,
      setLoadError,
    ]
  )
  const load = useCallback(async () => {
    if (transport !== getTransport()) return
    setLoading(true)
    const gen = remoteGenRef.current
    try {
      applyRead(await getComputerToolsSettings(), gen)
    } catch (e) {
      setLoadError(toErrorMessage(e))
    } finally {
      setLoading(false)
    }
  }, [transport, applyRead, setLoading, setLoadError])
  useEffect(() => {
    let cancelled = false
    const gen = remoteGenRef.current
    getComputerToolsSettings()
      .then((settings) => {
        if (!cancelled) applyRead(settings, gen)
      })
      .catch((e) => {
        if (!cancelled) setLoadError(toErrorMessage(e))
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [applyRead, setLoading, setLoadError])
  useEffect(() => {
    let disposed = false
    let unsubscribe: (() => void) | undefined
    void subscribe<ComputerToolsSettings>(
      COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT,
      (remote) => {
        if (disposed || transport !== getTransport()) return
        remoteGenRef.current += 1
        const next = fromSettings(remote)
        remoteRef.current = next
        const current = valuesRef.current
        const base = baselineRef.current
        setValues((prev) => ({
          ttl: ttlDirty(current, base) ? prev.ttl : next.ttl,
          blocklist: blocklistDirty(current, base)
            ? prev.blocklist
            : next.blocklist,
          removed: removedDirty(current, base) ? prev.removed : next.removed,
          stopShortcut: stopShortcutDirty(current, base)
            ? prev.stopShortcut
            : next.stopShortcut,
          showIndicator: showIndicatorDirty(current, base)
            ? prev.showIndicator
            : next.showIndicator,
          allowForeground: allowForegroundDirty(current, base)
            ? prev.allowForeground
            : next.allowForeground,
          defaultDelivery: defaultDeliveryDirty(current, base)
            ? prev.defaultDelivery
            : next.defaultDelivery,
          launchEnabled: launchEnabledDirty(current, base)
            ? prev.launchEnabled
            : next.launchEnabled,
          clipboardEnabled: clipboardEnabledDirty(current, base)
            ? prev.clipboardEnabled
            : next.clipboardEnabled,
          screenEnabled: screenEnabledDirty(current, base)
            ? prev.screenEnabled
            : next.screenEnabled,
        }))
        setBaseline(next)
        setEnabled(remote.enabled)
        setDefaults(remote.blocklistDefaults)
        setLoaded(true)
        setLoadError(null)
      }
    )
      .then((fn) => {
        if (disposed) fn()
        else unsubscribe = fn
      })
      .catch(() => {})
    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [
    transport,
    setValues,
    setBaseline,
    setEnabled,
    setDefaults,
    setLoaded,
    setLoadError,
  ])
  const dirtyTtl = ttlDirty(values, baseline)
  const dirtyBlocklist = blocklistDirty(values, baseline)
  const dirtyRemoved = removedDirty(values, baseline)
  const dirtyStopShortcut = stopShortcutDirty(values, baseline)
  const dirtyShowIndicator = showIndicatorDirty(values, baseline)
  const dirtyAllowForeground = allowForegroundDirty(values, baseline)
  const dirtyDefaultDelivery = defaultDeliveryDirty(values, baseline)
  const dirtyLaunchEnabled = launchEnabledDirty(values, baseline)
  const dirtyClipboardEnabled = clipboardEnabledDirty(values, baseline)
  const dirtyScreenEnabled = screenEnabledDirty(values, baseline)
  const dirty =
    dirtyTtl ||
    dirtyBlocklist ||
    dirtyRemoved ||
    dirtyStopShortcut ||
    dirtyShowIndicator ||
    dirtyAllowForeground ||
    dirtyDefaultDelivery ||
    dirtyLaunchEnabled ||
    dirtyClipboardEnabled ||
    dirtyScreenEnabled
  const editable = loaded && !saving
  const available = useComputerAvailable()
  const desktopHere = isLocalDesktop()
  const { isLinux: localLinux } = usePlatform()
  const isLinux = desktopHere
    ? localLinux
    : computerServerPlatform() === "linux"
  const save = useCallback(async () => {
    if (!loaded || transport !== getTransport()) return
    setSaving(true)
    const gen = remoteGenRef.current
    try {
      const applied = await setComputerToolsPreferences({
        grantTtlMinutes: dirtyTtl ? values.ttl : undefined,
        blocklist: dirtyBlocklist ? values.blocklist : undefined,
        blocklistRemoved: dirtyRemoved ? values.removed : undefined,
        stopShortcut: dirtyStopShortcut ? values.stopShortcut : undefined,
        showIndicator: dirtyShowIndicator ? values.showIndicator : undefined,
        allowForeground: dirtyAllowForeground
          ? values.allowForeground
          : undefined,
        defaultDelivery: dirtyDefaultDelivery
          ? values.defaultDelivery
          : undefined,
        launchEnabled: dirtyLaunchEnabled ? values.launchEnabled : undefined,
        clipboardEnabled: dirtyClipboardEnabled
          ? values.clipboardEnabled
          : undefined,
        screenEnabled: dirtyScreenEnabled ? values.screenEnabled : undefined,
      })
      if (transport !== getTransport()) return
      const latest =
        remoteGenRef.current !== gen && remoteRef.current
          ? remoteRef.current
          : fromSettings(applied)
      setValues(latest)
      setBaseline(latest)
      toast.success(t("saved"))
    } catch (e) {
      if (transport === getTransport())
        toast.error(t("saveFailed"), { description: toErrorMessage(e) })
    } finally {
      setSaving(false)
    }
  }, [
    transport,
    loaded,
    setSaving,
    setValues,
    setBaseline,
    values,
    dirtyTtl,
    dirtyBlocklist,
    dirtyRemoved,
    dirtyStopShortcut,
    dirtyShowIndicator,
    dirtyAllowForeground,
    dirtyDefaultDelivery,
    dirtyLaunchEnabled,
    dirtyClipboardEnabled,
    dirtyScreenEnabled,
    t,
  ])
  return {
    t,
    tComputer,
    loaded,
    loading,
    saving,
    loadError,
    values,
    setValues,
    baseline,
    enabled,
    defaults,
    load,
    dirtyStopShortcut,
    dirty,
    editable,
    available,
    desktopHere,
    isLinux,
    save,
  }
}
