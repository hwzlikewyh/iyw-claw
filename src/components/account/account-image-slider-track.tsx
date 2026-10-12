"use client"

import { ArrowRight, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { PUZZLE_MAX_POSITION } from "./account-puzzle-canvas"

interface Props {
  position: number
  disabled: boolean
  loading: boolean
  failed: boolean
  onChange: (position: number) => void
  onVerify: (position: number) => Promise<void>
}

export function AccountImageSliderTrack(props: Props) {
  const t = useTranslations("AccountFlow")
  const label = props.loading
    ? "sliderChecking"
    : props.failed
      ? "sliderRetry"
      : "sliderDrag"
  return (
    <>
      <div className="relative mt-3 h-10 rounded-lg border bg-background">
        <span
          className="pointer-events-none absolute inset-0 flex items-center justify-center gap-2 text-[11px] text-muted-foreground"
          role="status"
        >
          {props.loading ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <ArrowRight className="size-3" />
          )}
          {t(label)}
        </span>
        <SliderInput {...props} />
      </div>
      <p className="mt-2 text-center text-[10px] leading-relaxed text-muted-foreground">
        {t("sliderHint")}
      </p>
      <style>{`.account-image-slider::-webkit-slider-thumb{appearance:none;width:36px;height:36px;border:1px solid var(--primary);border-radius:8px;background:var(--primary);box-shadow:0 2px 5px #0001}.account-image-slider::-moz-range-thumb{width:34px;height:34px;border:1px solid var(--primary);border-radius:8px;background:var(--primary)}.account-image-slider:disabled{opacity:.55;cursor:wait}`}</style>
    </>
  )
}

function SliderInput(props: Props) {
  const t = useTranslations("AccountFlow")
  return (
    <input
      type="range"
      min={0}
      max={PUZZLE_MAX_POSITION}
      step={1}
      value={props.position}
      disabled={props.disabled}
      aria-label={t("sliderDrag")}
      aria-valuetext={t("sliderPosition", { position: props.position })}
      className="account-image-slider absolute inset-0 h-full w-full cursor-grab appearance-none bg-transparent touch-none focus-visible:outline-2 focus-visible:outline-ring"
      onChange={(event) => props.onChange(Number(event.target.value))}
      onPointerUp={(event) =>
        void props.onVerify(Number(event.currentTarget.value))
      }
      onPointerCancel={() => props.onChange(0)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") event.preventDefault()
      }}
      onKeyUp={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault()
          void props.onVerify(Number(event.currentTarget.value))
        }
      }}
    />
  )
}
