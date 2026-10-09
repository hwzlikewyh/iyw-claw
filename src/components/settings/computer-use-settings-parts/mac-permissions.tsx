"use client"

import { useTranslations } from "next-intl"
import {
  CircleAlert,
  CircleCheck,
  RotateCw,
  ShieldAlert,
  ShieldCheck,
} from "lucide-react"
import { SettingCard, SettingRow } from "@/components/computer/settings-layout"
import {
  SettingsError,
  SettingsSection,
} from "@/components/computer/settings-layout"
import { Button } from "@/components/ui/button"
import type { OsPermission } from "@/lib/computer/types"
import {
  hostHoldsPermission,
  useComputerStatus,
} from "@/lib/computer/use-computer-status"

export function MacPermissions({ enabled }: { enabled: boolean }) {
  const t = useTranslations("ComputerUse")
  const tp = useTranslations("ComputerUse.settings.permissions")
  const { status, loading, error, refresh, request, requesting, revealHelper } =
    useComputerStatus(enabled)
  const permissions = enabled ? status?.permissions : undefined
  const development = status?.backend.peer === "development"
  const missing =
    !!permissions && !(permissions.accessibility && permissions.screenRecording)
  const rows: ReadonlyArray<[OsPermission, boolean | undefined]> = [
    ["accessibility", permissions?.accessibility],
    ["screenRecording", permissions?.screenRecording],
  ]
  return (
    <SettingsSection
      icon={ShieldCheck}
      title={tp("title")}
      description={tp("description")}
      control={
        enabled ? (
          <Button
            size="xs"
            variant="ghost"
            onClick={() => void refresh()}
            disabled={loading}
            aria-label={t("refresh")}
            title={t("refresh")}
          >
            <RotateCw className={loading ? "size-3 animate-spin" : "size-3"} />
            {t("refresh")}
          </Button>
        ) : undefined
      }
    >
      {!enabled ? (
        <p className="text-xs text-muted-foreground">{tp("off")}</p>
      ) : (
        <>
          <SettingCard>
            {rows.map(([permission, granted]) => (
              <SettingRow
                key={permission}
                icon={
                  granted === undefined
                    ? undefined
                    : granted
                      ? CircleCheck
                      : CircleAlert
                }
                title={t(`permissions.${permission}`)}
                description={tp(`${permission}Hint`)}
                control={
                  granted === undefined ? null : granted ? (
                    <span className="text-xs text-muted-foreground">
                      {t("permissions.granted")}
                    </span>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={requesting !== null}
                      onClick={() => void request(permission)}
                    >
                      {t("permissions.request")}
                    </Button>
                  )
                }
              />
            ))}
          </SettingCard>
          {missing && (
            <div className="space-y-1 text-xs leading-5 text-muted-foreground">
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
        </>
      )}
      {hostHoldsPermission(status) && (
        <div className="flex gap-2 rounded-xl border border-amber-500/30 bg-amber-500/5 p-3 text-xs leading-5 text-amber-600 dark:text-amber-400">
          <ShieldAlert className="mt-0.5 size-3.5 shrink-0" />
          <span>{t("hostGranted")}</span>
        </div>
      )}
      {error && <SettingsError>{error}</SettingsError>}
    </SettingsSection>
  )
}
