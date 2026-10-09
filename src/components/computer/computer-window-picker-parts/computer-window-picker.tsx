"use client"

import { UnshareableWindows } from "./unshareable-windows"

import {
  AppWindow,
  ChevronDown,
  Eye,
  Layers,
  Loader2,
  MousePointerClick,
  RotateCw,
  TriangleAlert,
} from "lucide-react"
import { Button } from "@/components/ui/button"

import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Skeleton } from "@/components/ui/skeleton"

import { WindowTile } from "./window-tile"
import { ScreenCard } from "./screen-card"
import { LevelControl } from "./level-control"
import { GRID } from "../computer-window-picker"
import { useComputerWindowPickerState } from "./computer-window-picker-state"

export function ComputerWindowPicker(
  props: Parameters<typeof useComputerWindowPickerState>[0]
) {
  const {
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
  } = useComputerWindowPickerState(props)
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[min(calc(100dvh-2rem),52rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl">
        <div className="px-6 pt-6">
          <DialogHeader>
            <DialogTitle>{t("title")}</DialogTitle>
            <DialogDescription>{t("description")}</DialogDescription>
          </DialogHeader>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2 px-6 pt-5 pb-3">
          <p className="text-xs text-muted-foreground">
            {windows ? t("count", { count: shareable.length }) : t("loading")}
            {anyShared && (
              <span className="text-violet-600 dark:text-violet-400">
                {" · "}
                {t("sharedCount", { count: sharedCount })}
              </span>
            )}
          </p>
          <div className="flex items-center gap-1.5">
            {anyShared && (
              <Button
                size="sm"
                variant="destructive"
                onClick={() => void stopAll()}
                disabled={changing}
              >
                {t("stopSharingAll")}
              </Button>
            )}
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={changing || screenShared || shareable.length === 0}
                >
                  {bulk ? (
                    <Loader2 className="size-3.5 animate-spin" />
                  ) : (
                    <Layers className="size-3.5" />
                  )}
                  {t("shareAll")}
                  <ChevronDown className="size-3 opacity-60" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="min-w-56">
                <DropdownMenuItem onSelect={() => void shareAll("read")}>
                  <Eye className="size-3.5" />
                  {t("shareAllRead")}
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => void shareAll("control")}>
                  <MousePointerClick className="size-3.5" />
                  {t("shareAllControl")}
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <Button
              size="icon-sm"
              variant="ghost"
              onClick={() => void reload()}
              disabled={windows === null}
              title={t("refresh")}
              aria-label={t("refresh")}
            >
              <RotateCw className="size-3.5" />
            </Button>
          </div>
        </div>

        {(screenRecording === false || error || notice) && (
          <div className="space-y-2 px-6 pb-3">
            {screenRecording === false && (
              <div className="flex items-center gap-2.5 rounded-xl border border-amber-500/30 bg-amber-500/5 px-3 py-2">
                <TriangleAlert className="size-4 shrink-0 text-amber-500" />
                <div className="min-w-0 flex-1 space-y-1">
                  <p className="text-xs text-amber-700 dark:text-amber-400">
                    {t("noScreenRecording")}
                  </p>
                  {permissionError && (
                    <p className="text-2xs break-words text-red-500">
                      {permissionError}
                    </p>
                  )}
                </div>
                <Button
                  size="xs"
                  variant="outline"
                  className="shrink-0"
                  disabled={requesting !== null}
                  onClick={() => void request("screenRecording")}
                >
                  {tComputer("permissions.request")}
                </Button>
              </div>
            )}

            {error && (
              <div className="rounded-xl border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs break-words text-red-500">
                {error}
              </div>
            )}

            {notice && (
              <div className="rounded-xl border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
                {notice}
              </div>
            )}
          </div>
        )}

        <ScrollArea className="min-h-0 flex-1 border-t">
          <div className="space-y-4 px-6 pt-4 pb-6">
            {screenOffered && (
              <ScreenCard
                level={screenLevel}
                pending={busyScreen}
                disabled={changing}
                onLevel={(next) => void setScreenLevel(next)}
              />
            )}
            {windows === null ? (
              <div className={GRID} aria-hidden="true">
                {Array.from({ length: 4 }, (_, i) => (
                  <div key={i} className="overflow-hidden rounded-2xl border">
                    <Skeleton className="aspect-video w-full rounded-none" />
                    <div className="space-y-1.5 border-t px-2.5 py-2.5">
                      <Skeleton className="h-3 w-2/3 rounded-md" />
                      <Skeleton className="h-2.5 w-1/2 rounded-md" />
                    </div>
                  </div>
                ))}
              </div>
            ) : shareable.length === 0 ? (
              <div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed px-6 py-10 text-center text-sm text-muted-foreground">
                <AppWindow className="size-6 text-muted-foreground/50" />
                {t(windows.length === 0 ? "empty" : "noneShareable")}
              </div>
            ) : (
              <div className="space-y-5">
                {groups.map((group) => {
                  const app = appOf(group)
                  return (
                    <section
                      key={group.key}
                      aria-label={group.appName}
                      className="space-y-2"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5">
                        <p className="min-w-0 truncate text-xs font-medium">
                          {group.appName}
                          <span className="font-normal text-muted-foreground">
                            {" · "}
                            {t("count", { count: group.windows.length })}
                          </span>
                        </p>
                        <div
                          className="flex items-center gap-2"
                          title={t("wholeAppHint", { app: group.appName })}
                        >
                          <span className="text-2xs text-muted-foreground">
                            {t("wholeApp")}
                          </span>
                          <div className="w-48">
                            <LevelControl
                              label={t("appLevelLabel", {
                                app: group.appName,
                              })}
                              level={app.level}
                              pending={
                                busyApp?.key === group.key
                                  ? busyApp.level
                                  : null
                              }
                              disabled={changing || screenShared}
                              onLevel={(next) =>
                                void setAppLevel(group, app.appId, next)
                              }
                            />
                          </div>
                        </div>
                      </div>
                      <div className={GRID}>
                        {group.windows.map((w) => (
                          <WindowTile
                            key={w.targetId}
                            item={w}
                            level={levelOf(w)}
                            viaApp={stateOf(w).wholeApp}
                            viaScreen={stateOf(w).wholeScreen}
                            pending={
                              busy?.targetId === w.targetId ? busy.level : null
                            }
                            disabled={changing || screenShared}
                            pictures={pictures}
                            onLevel={(next) => void setLevel(w, next)}
                          />
                        ))}
                      </div>
                    </section>
                  )
                })}
              </div>
            )}

            <UnshareableWindows
              t={t}
              showUnshareable={showUnshareable}
              setShowUnshareable={setShowUnshareable}
              unshareable={unshareable}
            />
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  )
}
