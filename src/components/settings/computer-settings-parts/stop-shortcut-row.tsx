"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { Keyboard } from "lucide-react"
import { useIsMac } from "@/hooks/use-is-mac"
import { Button } from "@/components/ui/button"
import { SettingRow } from "@/components/computer/settings-layout"
import {
  defaultStopShortcut,
  spellStopShortcut,
  stopShortcutFromEvent,
  stopShortcutLabel,
  stopShortcutProblem,
  type StopShortcutProblem,
} from "@/lib/computer/stop-shortcut"
import { useComputerStopKey } from "@/lib/computer/use-stop-key"
import { setShortcutRecorderArmed } from "@/lib/keyboard-shortcuts"

export function StopShortcutRow({
  value,
  saved,
  enabled,
  disabled,
  onChange,
}: {
  value: string | null
  saved: string | null
  enabled: boolean
  disabled: boolean
  onChange: (value: string) => void
}) {
  const t = useTranslations("ComputerUse.settings.stopKey")
  const isMac = useIsMac()
  const status = useComputerStopKey()
  const [recording, setRecording] = useState(false)
  const [problem, setProblem] = useState<StopShortcutProblem | null>(null)
  const [wasDisabled, setWasDisabled] = useState(disabled)
  const fallback = defaultStopShortcut(isMac)
  if (disabled !== wasDisabled) {
    setWasDisabled(disabled)
    if (disabled) {
      setRecording(false)
      setProblem(null)
    }
  }
  useEffect(() => {
    if (!recording) return
    setShortcutRecorderArmed(true)
    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopPropagation()
      event.stopImmediatePropagation()
      if (event.repeat) return
      const parts = stopShortcutFromEvent(event)
      if (!parts) return
      const bare = !(parts.control || parts.alt || parts.shift || parts.command)
      if (bare && parts.code === "Escape") {
        setRecording(false)
        setProblem(null)
        return
      }
      const why = stopShortcutProblem(parts, isMac)
      if (why) {
        setProblem(why)
        return
      }
      setProblem(null)
      setRecording(false)
      onChange(spellStopShortcut(parts))
    }
    window.addEventListener("keydown", onKeyDown, true)
    return () => {
      window.removeEventListener("keydown", onKeyDown, true)
      setShortcutRecorderArmed(false)
    }
  }, [recording, isMac, onChange])
  let note: React.ReactNode = null
  if (recording) {
    note = problem
      ? t(`problem.${problem}`)
      : t(isMac ? "recordHintMac" : "recordHint")
  } else if (saved !== null) {
    if (saved === "") note = t("statusOff")
    else if (!enabled) note = t("statusIdle")
    else if (status?.active === saved) note = t("statusActive")
    else if (status?.failed === saved) note = t("statusFailed")
  }
  return (
    <SettingRow
      icon={Keyboard}
      title={t("label")}
      description={t("hint")}
      control={
        <div className="flex items-center gap-1">
          {!recording && value !== null && value !== fallback && (
            <Button
              size="xs"
              variant="ghost"
              onClick={() => onChange(fallback)}
              disabled={disabled}
            >
              {t("useDefault")}
            </Button>
          )}
          {!recording && value !== null && value !== "" && (
            <Button
              size="xs"
              variant="ghost"
              onClick={() => onChange("")}
              disabled={disabled}
            >
              {t("turnOff")}
            </Button>
          )}
          <Button
            size="sm"
            variant={recording ? "secondary" : "outline"}
            className="min-w-28 font-mono"
            aria-pressed={recording}
            onClick={() => {
              setProblem(null)
              setRecording((r) => !r)
            }}
            disabled={disabled}
          >
            {recording
              ? t("recording")
              : value === null
                ? "…"
                : value
                  ? stopShortcutLabel(value, isMac)
                  : t("off")}
          </Button>
        </div>
      }
    >
      {note && (
        <p
          className={
            problem || (!recording && status?.failed === saved && saved)
              ? "text-xs text-destructive"
              : "text-xs text-muted-foreground"
          }
          title={
            !recording && status?.failed === saved ? status?.detail : undefined
          }
        >
          {note}
        </p>
      )}
    </SettingRow>
  )
}
