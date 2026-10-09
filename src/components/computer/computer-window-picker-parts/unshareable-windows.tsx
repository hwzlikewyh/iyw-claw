"use client"

import { ChevronRight, ShieldOff } from "lucide-react"

import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"

import { cn } from "@/lib/utils"

import { useComputerWindowPickerState } from "./computer-window-picker-state"

export function UnshareableWindows({
  t,
  showUnshareable,
  setShowUnshareable,
  unshareable,
}: Pick<
  ReturnType<typeof useComputerWindowPickerState>,
  "t" | "showUnshareable" | "setShowUnshareable" | "unshareable"
>) {
  return (
    <>
      {unshareable.length > 0 && (
        <Collapsible open={showUnshareable} onOpenChange={setShowUnshareable}>
          <CollapsibleTrigger asChild>
            <button
              type="button"
              className="flex items-center gap-1 text-xs text-muted-foreground transition-colors hover:text-foreground"
            >
              <ChevronRight
                className={cn(
                  "size-3.5 transition-transform rtl:rotate-180",
                  showUnshareable && "rotate-90 rtl:rotate-90"
                )}
              />
              {t("unshareable", { count: unshareable.length })}
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <ul className="mt-2 divide-y overflow-hidden rounded-xl border">
              {unshareable.map((w) => (
                <li
                  key={w.targetId}
                  className="flex items-center gap-2.5 px-3 py-2"
                >
                  <ShieldOff className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-xs">
                    <span className="font-medium">
                      {w.appName || t("unnamedApp")}
                    </span>

                    {w.title && w.title !== w.appName && (
                      <span className="text-muted-foreground" title={w.title}>
                        {" · "}
                        {w.title}
                      </span>
                    )}
                  </span>
                  <span className="shrink-0 text-2xs text-muted-foreground">
                    {t(`notGrantable.${w.notGrantable}`)}
                  </span>
                </li>
              ))}
            </ul>
            {unshareable.some((w) => w.notGrantable === "own-app") && (
              <p className="mt-2 text-2xs leading-snug text-muted-foreground">
                {t("ownAppWhy")}
              </p>
            )}
          </CollapsibleContent>
        </Collapsible>
      )}
    </>
  )
}
