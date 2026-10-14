"use client"

import { useTranslations } from "next-intl"
import { list, record, text, useViewState } from "./context"
import { CartesianChart } from "./chart-cartesian"
import { SpecialChart } from "./chart-special"
import {
  chartModel,
  chartColor,
  CHART_WIDTH,
  CHART_HEIGHT,
} from "./chart-model"

export function Chart({ props }: { props: Record<string, unknown> }) {
  const t = useTranslations("IntelligentUI")
  const [hidden, setHidden] = useViewState<number[]>("hidden", [])
  const series = list(props.series).slice(0, 12).map(record)
  const model = chartModel(props, hidden)
  const specialized = ["pie", "radar", "radial", "funnel"].includes(model.type)
  return (
    <div className="space-y-2">
      {model.series.length ? (
        <svg
          viewBox={`0 0 ${CHART_WIDTH} ${CHART_HEIGHT}`}
          className="max-h-80 w-full"
          role="img"
          aria-label={text(props.title ?? t("chart"))}
        >
          {specialized ? (
            <SpecialChart model={model} />
          ) : (
            <CartesianChart model={model} />
          )}
        </svg>
      ) : (
        <p className="text-xs text-muted-foreground">{t("empty")}</p>
      )}
      <div className="flex flex-wrap gap-2">
        {series.map((s, i) => (
          <button
            key={i}
            type="button"
            aria-pressed={!hidden.includes(i)}
            className="flex items-center gap-1 rounded px-1 py-0.5 text-xs focus-visible:ring-2 focus-visible:ring-ring"
            onClick={() =>
              setHidden(
                hidden.includes(i)
                  ? hidden.filter((value) => value !== i)
                  : [...hidden, i]
              )
            }
          >
            <span
              style={{ background: chartColor(i) }}
              className={`size-2 rounded ${hidden.includes(i) ? "opacity-30" : ""}`}
            />
            {text(s.name ?? i + 1)}
          </button>
        ))}
      </div>
    </div>
  )
}
