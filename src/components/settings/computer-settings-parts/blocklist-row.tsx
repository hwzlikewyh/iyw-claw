"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Plus, RotateCcw, X } from "lucide-react"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { SettingRow } from "@/components/computer/settings-layout"
import { Input } from "@/components/ui/input"
import { type DefaultBlock } from "@/lib/computer/types"
import { SYSTEM_ENTRY_NAMES } from "../computer-settings"

export function BlocklistRow({
  defaults,
  blocklist,
  removed,
  disabled,
  onChange,
}: {
  defaults: readonly DefaultBlock[]
  blocklist: readonly string[]
  removed: readonly string[]
  disabled: boolean
  onChange: (blocklist: string[], removed: string[]) => void
}) {
  const t = useTranslations("ComputerUse.settings")
  const [draft, setDraft] = useState("")
  const [duplicate, setDuplicate] = useState(false)
  const [confirmRestore, setConfirmRestore] = useState(false)
  const shown = defaults.filter((entry) => !removed.includes(entry.key))
  const removedCount = defaults.length - shown.length
  const atDefaults = blocklist.length === 0 && removedCount === 0
  const nameOf = (entry: DefaultBlock) =>
    entry.key in SYSTEM_ENTRY_NAMES
      ? t(SYSTEM_ENTRY_NAMES[entry.key as keyof typeof SYSTEM_ENTRY_NAMES])
      : entry.name
  const add = () => {
    const entry = draft.trim()
    if (!entry) return
    const lower = entry.toLowerCase()
    const known = defaults.find(
      (d) =>
        d.names.some((name) => name.toLowerCase() === lower) ||
        d.name.toLowerCase() === lower ||
        nameOf(d).toLowerCase() === lower
    )
    if (known && removed.includes(known.key)) {
      onChange(
        [...blocklist],
        removed.filter((key) => key !== known.key)
      )
      setDraft("")
      return
    }
    if (known || blocklist.some((e) => e.toLowerCase() === lower)) {
      setDuplicate(true)
      return
    }
    onChange([...blocklist, entry], [...removed])
    setDraft("")
  }
  return (
    <AlertDialog open={confirmRestore} onOpenChange={setConfirmRestore}>
      <SettingRow
        title={t("blocklist.label")}
        description={t("blocklist.hint")}
        htmlFor="computer-blocklist"
        control={
          <AlertDialogTrigger asChild>
            <Button size="xs" variant="ghost" disabled={disabled || atDefaults}>
              <RotateCcw className="size-3" />
              {t("blocklist.restore")}
            </Button>
          </AlertDialogTrigger>
        }
      >
        <div className="space-y-2">
          <ul className="divide-y overflow-hidden rounded-lg border">
            {shown.map((entry) => (
              <li
                key={entry.key}
                className="flex items-center justify-between gap-3 px-3 py-1.5"
              >
                <span className="min-w-0">
                  <span className="block truncate text-sm">
                    {nameOf(entry)}
                  </span>
                  <span
                    className="block truncate font-mono text-2xs text-muted-foreground"
                    title={entry.names.join("\n")}
                  >
                    {entry.names.join(" · ")}
                  </span>
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="size-7 shrink-0"
                  disabled={disabled}
                  title={t("blocklist.remove", { name: nameOf(entry) })}
                  aria-label={t("blocklist.remove", { name: nameOf(entry) })}
                  onClick={() =>
                    onChange([...blocklist], [...removed, entry.key])
                  }
                >
                  <X className="size-3.5" />
                </Button>
              </li>
            ))}
            {blocklist.map((entry) => (
              <li
                key={entry}
                className="flex items-center justify-between gap-3 px-3 py-1.5"
              >
                <span className="min-w-0">
                  <span className="block truncate font-mono text-xs">
                    {entry}
                  </span>
                  <span className="block truncate text-2xs text-muted-foreground">
                    {t("blocklist.custom")}
                  </span>
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="size-7 shrink-0"
                  disabled={disabled}
                  title={t("blocklist.remove", { name: entry })}
                  aria-label={t("blocklist.remove", { name: entry })}
                  onClick={() =>
                    onChange(
                      blocklist.filter((e) => e !== entry),
                      [...removed]
                    )
                  }
                >
                  <X className="size-3.5" />
                </Button>
              </li>
            ))}
          </ul>
          <div className="flex items-center gap-2">
            <Input
              id="computer-blocklist"
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value)
                setDuplicate(false)
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.nativeEvent.isComposing) {
                  e.preventDefault()
                  add()
                }
              }}
              placeholder={t("blocklist.placeholder")}
              disabled={disabled}
              className="h-8 font-mono text-xs"
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={add}
              disabled={disabled || !draft.trim()}
            >
              <Plus className="size-3.5" />
              {t("blocklist.add")}
            </Button>
          </div>
          {duplicate && (
            <p className="text-xs text-destructive">
              {t("blocklist.duplicate")}
            </p>
          )}
          {removedCount > 0 && (
            <p className="text-xs text-muted-foreground">
              {t("blocklist.removedCount", { count: removedCount })}
            </p>
          )}
        </div>

        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("blocklist.restoreConfirmTitle")}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {blocklist.length > 0 && (
                <span className="block">
                  {t("blocklist.restoreConfirmAdded", {
                    count: blocklist.length,
                  })}
                </span>
              )}
              {removedCount > 0 && (
                <span className="block">
                  {t("blocklist.restoreConfirmRemoved", {
                    count: removedCount,
                  })}
                </span>
              )}
              <span className="block">{t("blocklist.restoreConfirmSave")}</span>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("blocklist.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              disabled={disabled}
              onClick={() => {
                onChange([], [])
                setDuplicate(false)
              }}
            >
              {t("blocklist.restore")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </SettingRow>
    </AlertDialog>
  )
}
