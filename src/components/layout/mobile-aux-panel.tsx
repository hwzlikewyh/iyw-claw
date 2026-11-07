"use client"

import { useTranslations } from "next-intl"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { AuxPanel } from "./aux-panel"
import { AuxPanelSurface } from "./aux-panel-surface"

export function MobileAuxPanel({
  mobile,
  desktopSlot,
}: {
  mobile: boolean
  desktopSlot: HTMLElement | null
}) {
  const { isOpen, restored, setOpen } = useAuxPanelContext()
  const t = useTranslations("Folder.auxPanel.tabs")
  return (
    <AuxPanelSurface
      mobile={mobile}
      desktopSlot={desktopSlot}
      open={restored && isOpen}
      title={t("label")}
      onOpenChange={setOpen}
    >
      <AuxPanel />
    </AuxPanelSurface>
  )
}
