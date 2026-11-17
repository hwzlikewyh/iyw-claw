"use client"

import { useTranslations } from "next-intl"
import {
  Loader2,
  Maximize2,
  Minimize2,
  SquareArrowOutUpRight,
  X,
} from "lucide-react"
import { Button } from "@/components/ui/button"

export function SyncError({ refresh }: { refresh?: () => void }) {
  const t = useTranslations("PluginPanels")
  return (
    <div
      role="status"
      className="flex items-center justify-between gap-2 px-3 py-2 text-xs text-muted-foreground"
    >
      <span>{t("syncFailed")}</span>
      <Button size="xs" variant="link" onClick={refresh}>
        {t("refresh")}
      </Button>
    </div>
  )
}

export function PanelToolbar({
  label,
  fullscreen,
  toggle,
  close,
  detach,
  moving,
}: {
  label: string
  fullscreen: boolean
  toggle: () => void
  close?: () => void
  detach?: () => void
  moving?: boolean
}) {
  const t = useTranslations("PluginPanels")
  return (
    <div className="flex h-8 shrink-0 items-center justify-between border-b px-2 max-md:h-11">
      <span className="truncate text-xs">{label}</span>
      <div className="flex shrink-0 items-center gap-1">
        {detach && <DetachButton onClick={detach} moving={moving} />}
        {close && (
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t("closeApp", { name: label })}
            onClick={close}
          >
            <X />
          </Button>
        )}
        <Button
          variant="ghost"
          size="icon-xs"
          className="max-md:size-11"
          aria-label={t(fullscreen ? "restore" : "maximize")}
          onClick={toggle}
        >
          {fullscreen ? <Minimize2 /> : <Maximize2 />}
        </Button>
      </div>
    </div>
  )
}

function DetachButton({
  onClick,
  moving,
}: {
  onClick: () => void
  moving?: boolean
}) {
  const t = useTranslations("PluginPanels")
  const label = t(moving ? "openingWindow" : "detachApp")
  return (
    <Button
      variant="ghost"
      size="icon-xs"
      disabled={moving}
      aria-busy={moving}
      title={label}
      aria-label={label}
      onClick={onClick}
    >
      {moving ? (
        <Loader2 className="animate-spin" />
      ) : (
        <SquareArrowOutUpRight />
      )}
    </Button>
  )
}
