"use client"

import { Loader2, Monitor, Square } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { DropdownMenuItem } from "@/components/ui/dropdown-menu"
import { SettingsSection } from "@/components/computer/settings-layout"
import { ComputerWindowPicker } from "./computer-window-picker"
import {
  useComputerSharing,
  type ComputerSharingControl,
} from "@/lib/computer/use-computer-sharing"

export function ComputerSharingMenuItems({
  control,
}: {
  control: ComputerSharingControl
}) {
  const t = useTranslations("ComputerUse")
  if (!control.available || control.enabled !== true) return null
  return (
    <>
      <DropdownMenuItem
        disabled={
          !control.available || control.enabled === null || control.pending
        }
        onSelect={() => void control.begin()}
      >
        {control.pending ? (
          <Loader2 className="size-4 animate-spin" />
        ) : (
          <Monitor className="size-4" />
        )}
        {control.label}
      </DropdownMenuItem>
      {control.count > 0 && (
        <DropdownMenuItem
          disabled={control.pending}
          onSelect={() => void control.stop()}
        >
          <Square className="size-4" />
          {t("stop")}
        </DropdownMenuItem>
      )}
    </>
  )
}

export function ComputerSharingSection() {
  const t = useTranslations("ComputerUse")
  const control = useComputerSharing()
  if (!control.available || control.enabled !== true) return null
  return (
    <>
      <SettingsSection
        icon={Monitor}
        title={t("sharingTitle")}
        description={t("sharingHint")}
      >
        <div className="flex flex-wrap items-center gap-2">
          <Button
            size="sm"
            disabled={
              !control.available || control.enabled === null || control.pending
            }
            onClick={() => void control.begin()}
          >
            {control.pending ? (
              <Loader2 className="size-4 animate-spin" />
            ) : (
              <Monitor className="size-4" />
            )}
            {control.label}
          </Button>
          {control.count > 0 && (
            <Button
              size="sm"
              variant="destructive"
              disabled={control.pending}
              onClick={() => void control.stop()}
            >
              <Square className="size-4" />
              {t("stop")}
            </Button>
          )}
        </div>
      </SettingsSection>
      <ComputerWindowPicker
        open={control.open}
        onOpenChange={control.setOpen}
      />
    </>
  )
}
