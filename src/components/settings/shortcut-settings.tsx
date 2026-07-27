"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Keyboard, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useIsMac } from "@/hooks/use-is-mac"
import { useShortcutSettings } from "@/hooks/use-shortcut-settings"
import {
  DEFAULT_SHORTCUTS,
  INPUT_SHORTCUT_IDS,
  SHORTCUT_DEFINITIONS,
  type ShortcutActionId,
  formatShortcutLabel,
  shortcutFromKeyboardEvent,
} from "@/lib/keyboard-shortcuts"
import { Button } from "@/components/ui/button"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"

export function ShortcutSettings() {
  const t = useTranslations("ShortcutSettings")
  const { shortcuts, updateShortcut, resetShortcuts } = useShortcutSettings()
  const isMac = useIsMac()
  const [recordingAction, setRecordingAction] =
    useState<ShortcutActionId | null>(null)
  const actionTitle = useCallback(
    (id: ShortcutActionId) => t(`actions.${id}.title`),
    [t]
  )
  const actionDescription = useCallback(
    (id: ShortcutActionId) => t(`actions.${id}.description`),
    [t]
  )

  const isDefault = useMemo(
    () =>
      SHORTCUT_DEFINITIONS.every(
        (definition) =>
          shortcuts[definition.id] === DEFAULT_SHORTCUTS[definition.id]
      ),
    [shortcuts]
  )

  useEffect(() => {
    if (!recordingAction) return

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) return
      event.preventDefault()
      event.stopPropagation()

      if (event.key === "Escape") {
        setRecordingAction(null)
        return
      }

      const allowNoModifier = INPUT_SHORTCUT_IDS.has(recordingAction)
      const shortcut = shortcutFromKeyboardEvent(event, allowNoModifier)
      if (!shortcut) return

      const conflict = SHORTCUT_DEFINITIONS.find(
        (definition) =>
          definition.id !== recordingAction &&
          shortcuts[definition.id] === shortcut
      )

      if (conflict) {
        toast.error(t("toasts.conflict", { title: actionTitle(conflict.id) }))
        return
      }

      if (updateShortcut(recordingAction, shortcut)) {
        toast.success(t("toasts.updated"))
      } else {
        toast.error(t("toasts.invalid"))
      }

      setRecordingAction(null)
    }

    window.addEventListener("keydown", onKeyDown, true)

    return () => {
      window.removeEventListener("keydown", onKeyDown, true)
    }
  }, [actionTitle, recordingAction, shortcuts, t, updateShortcut])

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={Keyboard}
        title={t("sectionTitle")}
        description={t("recordInstruction")}
        action={
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              resetShortcuts()
              setRecordingAction(null)
              toast.success(t("toasts.reset"))
            }}
            disabled={isDefault}
          >
            <RotateCcw className="h-3.5 w-3.5" />
            {t("resetDefault")}
          </Button>
        }
      />

      <section className="overflow-hidden rounded-xl border bg-card">
        <div className="divide-y divide-border/60">
          {SHORTCUT_DEFINITIONS.map((definition) => {
            const isRecording = recordingAction === definition.id
            return (
              <div
                key={definition.id}
                className="flex items-center justify-between gap-4 px-4 py-3"
              >
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium">
                    {actionTitle(definition.id)}
                  </div>
                  <p className="mt-0.5 text-xs text-muted-foreground truncate">
                    {actionDescription(definition.id)}
                  </p>
                </div>
                <Button
                  variant={isRecording ? "default" : "secondary"}
                  size="sm"
                  className="font-mono min-w-36 justify-center shrink-0"
                  onClick={() => {
                    setRecordingAction((previous) =>
                      previous === definition.id ? null : definition.id
                    )
                  }}
                >
                  {isRecording
                    ? t("recording")
                    : formatShortcutLabel(shortcuts[definition.id], isMac)}
                </Button>
              </div>
            )
          })}
        </div>
      </section>
    </SettingsPageLayout>
  )
}
