"use client"

import { CircleAlert, CircleCheck } from "lucide-react"
import { Button } from "@/components/ui/button"

import { useComputerPopoverState } from "./computer-popover-state"

export function ComputerPermissionRows({
  t,
  request,
  requesting,
  revealHelper,
  permissions,
  development,
}: Pick<
  ReturnType<typeof useComputerPopoverState>,
  | "t"
  | "request"
  | "requesting"
  | "revealHelper"
  | "permissions"
  | "development"
>) {
  return (
    <>
      {permissions?.required && (
        <div className="divide-y overflow-hidden rounded-lg border">
          {(
            [
              ["accessibility", permissions.accessibility],
              ["screenRecording", permissions.screenRecording],
            ] as const
          ).map(([permission, granted]) => (
            <div
              key={permission}
              className="flex items-center gap-2 px-2 py-1.5"
            >
              {granted ? (
                <CircleCheck className="size-3.5 shrink-0 text-emerald-500" />
              ) : (
                <CircleAlert className="size-3.5 shrink-0 text-amber-500" />
              )}
              <span className="min-w-0 flex-1 truncate text-2xs font-medium">
                {t(`permissions.${permission}`)}
              </span>
              {granted ? (
                <span className="text-3xs text-muted-foreground">
                  {t("permissions.granted")}
                </span>
              ) : (
                <Button
                  size="xs"
                  variant="outline"
                  disabled={requesting !== null}
                  onClick={() => void request(permission)}
                >
                  {t("permissions.request")}
                </Button>
              )}
            </div>
          ))}
          {!(permissions.accessibility && permissions.screenRecording) && (
            <div className="space-y-1 px-2 py-1.5 text-3xs leading-snug text-muted-foreground">
              <p>
                {t("permissions.why")}
                {development && ` ${t("permissions.devRebuild")}`}
              </p>
              <p>
                {t("permissions.notListed")}{" "}
                <button
                  type="button"
                  className="underline underline-offset-2 hover:text-foreground"
                  onClick={revealHelper}
                >
                  {t("permissions.reveal")}
                </button>
              </p>
            </div>
          )}
        </div>
      )}
    </>
  )
}
