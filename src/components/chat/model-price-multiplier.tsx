"use client"

const MIN_DECIMAL_MULTIPLIER = 0.000001
const LARGE_MULTIPLIER = 1000
const multiplierFormatter = new Intl.NumberFormat("en-US", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 6,
  useGrouping: false,
})

export function ModelPriceMultiplier({ value }: { value?: number | null }) {
  if (value == null || !Number.isFinite(value) || value < 0) return null

  const formatted =
    (value > 0 && value < MIN_DECIMAL_MULTIPLIER) || value >= LARGE_MULTIPLIER
      ? value.toExponential(2)
      : multiplierFormatter.format(value)
  const label = `${formatted}x`

  return (
    <span
      className="inline-flex w-20 shrink-0 justify-end text-xs text-muted-foreground tabular-nums whitespace-nowrap"
      title={`${value}x`}
      aria-label={label}
    >
      {label}
    </span>
  )
}
