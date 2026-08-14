"use client"

import { useRef } from "react"
import { LoaderCircle, MonitorX } from "lucide-react"
import { useTranslations } from "next-intl"
import { useBrowserFrames } from "@/hooks/use-browser-frames"
import { useBrowserInput } from "@/hooks/use-browser-input"
import type {
  BrowserTabSnapshot,
  BrowserViewClaimSnapshot,
} from "@/lib/browser-types"

export function BrowserCanvas({
  tab,
  claim,
}: {
  tab: BrowserTabSnapshot | null
  claim?: BrowserViewClaimSnapshot
}) {
  const t = useTranslations("Browser")
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const textInputRef = useRef<HTMLTextAreaElement>(null)
  const { subscription, error } = useBrowserFrames(canvasRef, tab, claim)
  const { canvasProps, textProps } = useBrowserInput(canvasRef, subscription)

  const focusInput = () => textInputRef.current?.focus({ preventScroll: true })

  return (
    <div className="relative h-full min-h-0 overflow-hidden bg-white dark:bg-neutral-950">
      <canvas
        ref={canvasRef}
        className="block h-full w-full touch-none select-none outline-none"
        {...canvasProps}
        onPointerDown={(event) => {
          focusInput()
          canvasProps.onPointerDown(event)
        }}
      />
      <textarea
        ref={textInputRef}
        className="pointer-events-none absolute left-0 top-0 h-px w-px resize-none overflow-hidden opacity-0"
        aria-label={t("canvasInput")}
        autoCapitalize="off"
        autoCorrect="off"
        spellCheck={false}
        {...textProps}
      />
      {!tab ? (
        <CanvasState icon={MonitorX} label={t("emptyTab")} />
      ) : tab.status !== "live" ? (
        <CanvasState
          icon={tab.status === "crashed" ? MonitorX : LoaderCircle}
          label={t(`tabStatus.${tab.status}`)}
          spin={tab.status === "creating" || tab.status === "navigating"}
        />
      ) : !subscription ? (
        <CanvasState icon={LoaderCircle} label={t("connecting")} spin />
      ) : null}
      {error ? (
        <div className="absolute inset-x-0 bottom-0 bg-destructive px-3 py-1.5 text-xs text-destructive-foreground">
          {t("streamDisconnected")}
        </div>
      ) : null}
    </div>
  )
}

function CanvasState({
  icon: Icon,
  label,
  spin = false,
}: {
  icon: typeof LoaderCircle
  label: string
  spin?: boolean
}) {
  return (
    <div className="absolute inset-0 flex items-center justify-center bg-background text-muted-foreground">
      <div className="flex items-center gap-2 text-sm">
        <Icon className={`size-4 ${spin ? "animate-spin" : ""}`} />
        <span>{label}</span>
      </div>
    </div>
  )
}
