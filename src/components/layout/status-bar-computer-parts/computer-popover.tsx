"use client"

import { ComputerPermissionRows } from "./computer-permission-rows"

import {
  BrushCleaning,
  Monitor,
  RotateCw,
  Settings2,
  ShieldAlert,
  Square,
} from "lucide-react"
import { Button } from "@/components/ui/button"

import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { ComputerWindowPicker } from "@/components/computer/computer-window-picker"
import { openSettingsWindow } from "@/lib/api"

import { cn } from "@/lib/utils"
import { formatTime } from "./format-time"
import { SharedRow } from "./shared-row"
import { AGENT_MARK, ACTIVITY_SHOWN } from "../status-bar-computer"
import { useComputerPopoverState } from "./computer-popover-state"

export function ComputerPopover() {
  const {
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
  } = useComputerPopoverState()
  return (
    <>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <button
            aria-label={t("title")}
            title={
              anyShared
                ? t("tooltipShared", { count: shared.length })
                : t("title")
            }
            className="relative flex items-center transition-colors hover:text-foreground"
          >
            <Monitor className={cn("size-3.5", anyShared && AGENT_MARK)} />
          </button>
        </PopoverTrigger>
        <PopoverContent
          ref={contentRef}
          side="top"
          align="end"
          className="w-88 gap-2 p-2.5"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-1.5 truncate text-xs font-medium">
              {t("title")}
              {!status?.verifiedPlatform && (
                <span className="rounded-full bg-muted px-1.5 py-0.5 text-3xs font-medium text-muted-foreground">
                  {t("preview")}
                </span>
              )}
            </span>
            <button
              type="button"
              onClick={() => void refresh()}
              title={t("refresh")}
              aria-label={t("refresh")}
              className="text-muted-foreground transition-colors hover:text-foreground"
            >
              <RotateCw className={cn("h-3 w-3", loading && "animate-spin")} />
            </button>
          </div>

          {anyShared && (
            <Button
              size="sm"
              variant="destructive"
              className="w-full"
              onClick={() => void stopSharing()}
              disabled={stopping}
              title={t("stopHint")}
            >
              <Square className="size-3 fill-current" />
              {t("stop")}
              {shortcut && (
                <kbd className="font-sans text-2xs opacity-80">{shortcut}</kbd>
              )}
            </Button>
          )}

          {hostLeaks && (
            <div className="flex gap-1.5 rounded-md border border-amber-500/30 bg-amber-500/5 px-2 py-1.5 text-2xs text-amber-600 dark:text-amber-400">
              <ShieldAlert className="mt-0.5 size-3.5 shrink-0" />
              <span>{t("hostGranted")}</span>
            </div>
          )}

          {liveBackend && (
            <p className="text-2xs text-muted-foreground">
              {t(`backend.${liveBackend.state}`)}
              {" · "}
              {t("driver", { version: liveBackend.driverVersion })}
              {development && (
                <span
                  className="text-amber-600 dark:text-amber-400"
                  title={t("devBuild")}
                >
                  {" · "}
                  {t("devTag")}
                </span>
              )}
              {liveBackend.detail ? ` — ${liveBackend.detail}` : ""}
            </p>
          )}

          <ComputerPermissionRows
            t={t}
            request={request}
            requesting={requesting}
            revealHelper={revealHelper}
            permissions={permissions}
            development={development}
          />

          <div className="overflow-hidden rounded-lg border">
            <div className="px-2 py-1.5">
              <span className="text-2xs font-medium">
                {t("shared.title", { count: shared.length })}
              </span>
            </div>
            {rows === 0 ? (
              <p className="border-t px-2 py-1.5 text-3xs text-muted-foreground">
                {t("shared.empty")}
              </p>
            ) : (
              <div className="divide-y border-t">
                {sharedScreen && (
                  <SharedRow
                    screen
                    name={t("shared.screen")}
                    detail={t("shared.screenWindows", {
                      count: sharedScreen.windows,
                    })}
                    level={sharedScreen.level}
                    onLevel={(level) => void setScreenLevel(level)}
                  />
                )}
                {sharedApps.map((a) => (
                  <SharedRow
                    key={a.appId}
                    whole
                    name={a.appName}
                    detail={t("shared.wholeApp", { count: a.windows })}
                    level={a.level}
                    onLevel={(level) => void setAppLevel(a.appId, level)}
                  />
                ))}
                {ownWindows.map((w) => (
                  <SharedRow
                    key={w.targetId}
                    name={w.appName}
                    detail={w.title}
                    level={w.level}
                    onLevel={(level) => void setLevel(w.targetId, level)}
                  />
                ))}
              </div>
            )}
          </div>

          {activity.length > 0 && (
            <div className="rounded-lg border px-2 py-1.5">
              <div className="mb-1 flex items-center justify-between gap-2">
                <p className="text-2xs font-medium">{t("activity.title")}</p>
                <button
                  type="button"
                  onClick={clearActivity}
                  title={t("activity.clear")}
                  aria-label={t("activity.clear")}
                  className="text-muted-foreground transition-colors hover:text-foreground"
                >
                  <BrushCleaning className="h-3 w-3" />
                </button>
              </div>
              <ul className="space-y-0.5">
                {activity.slice(0, ACTIVITY_SHOWN).map((line, i) => (
                  <li
                    key={`${line.at}-${i}`}
                    className="flex items-center gap-1.5 text-3xs text-muted-foreground"
                  >
                    <span className="tabular-nums">{formatTime(line.at)}</span>
                    <span className="min-w-0 flex-1 truncate">
                      {t(`activity.${line.action}`)}
                      {line.actor ? ` · ${line.actor.agent}` : ""}
                      {appNameOf(line) ? ` · ${appNameOf(line)}` : ""}
                      {line.count > 1 ? ` ×${line.count}` : ""}
                    </span>
                    <span
                      className={cn(
                        line.outcome === "done"
                          ? "text-emerald-600 dark:text-emerald-400"
                          : line.outcome === "refused"
                            ? "text-amber-600 dark:text-amber-400"
                            : "text-red-500"
                      )}
                    >
                      {t(`activity.outcome.${line.outcome}`)}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {error && (
            <div className="rounded-md border border-red-500/30 bg-red-500/5 px-2 py-1.5 text-2xs break-words text-red-500">
              {error}
            </div>
          )}

          <Button
            size="sm"
            className="w-full"
            onClick={() => {
              setOpen(false)
              setPickerOpen(true)
            }}
          >
            <Monitor className="h-3.5 w-3.5" />
            {t("share")}
          </Button>
          <Button
            size="sm"
            variant="outline"
            className="w-full"
            onClick={() => {
              openSettingsWindow("computer-use").catch((err) => {
                console.error(
                  "[StatusBarComputer] failed to open settings:",
                  err
                )
              })
            }}
          >
            <Settings2 className="h-3.5 w-3.5" />
            {t("openSettings")}
          </Button>
        </PopoverContent>
      </Popover>
      <ComputerWindowPicker open={pickerOpen} onOpenChange={setPickerOpen} />
    </>
  )
}
