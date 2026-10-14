"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import {
  parseExpression,
  createBudget,
} from "@/lib/intelligent-ui/engine/expression"
import { Chart } from "./chart"
import { list, record, numeric, useViewState } from "./context"

const SAMPLES = 160,
  MAX_FUNCTIONS = 8,
  EXPRESSION_LENGTH = 400
export function FunctionPlot({ props }: { props: Record<string, unknown> }) {
  const t = useTranslations("IntelligentUI")
  const [expressions, setExpressions] = useViewState<string[]>(
    "expressions",
    list(props.functions)
      .slice(0, MAX_FUNCTIONS)
      .map((value) => String(record(value).expr ?? ""))
  )
  const initial = [
    numeric(list(props.x)[0], -10),
    numeric(list(props.x)[1], 10),
  ]
  const [viewport, setViewport] = useViewState<number[]>("viewport", initial)
  const min = viewport[0],
    max = Math.max(min + 1e-6, viewport[1])
  const plot = useMemo(
    () => sampleFunctions(expressions, min, max),
    [expressions, min, max]
  )
  return (
    <div className="space-y-2">
      <ExpressionInputs expressions={expressions} update={setExpressions} />
      <PlotToolbar
        expressions={expressions}
        initial={initial}
        viewport={[min, max]}
        changeExpressions={setExpressions}
        changeViewport={setViewport}
      />
      {plot.invalid.length > 0 && (
        <p role="status" className="text-xs text-destructive">
          {t("invalidExpression", { expressions: plot.invalid.join(", ") })}
        </p>
      )}
      <Chart
        props={{ type: "line", x: plot.xs, y: props.y, series: plot.series }}
      />
    </div>
  )
}
function ExpressionInputs({
  expressions,
  update,
}: {
  expressions: string[]
  update: (values: string[]) => void
}) {
  const t = useTranslations("IntelligentUI")
  return expressions.map((expr, index) => (
    <div key={index} className="flex gap-1">
      <Input
        aria-label={t("expression")}
        value={expr}
        maxLength={EXPRESSION_LENGTH}
        onChange={(event) =>
          update(
            expressions.map((value, i) =>
              i === index ? event.target.value : value
            )
          )
        }
      />
      <Button
        variant="ghost"
        aria-label={t("remove")}
        onClick={() => update(expressions.filter((_, i) => i !== index))}
      >
        ×
      </Button>
    </div>
  ))
}
function sampleFunctions(expressions: string[], min: number, max: number) {
  const xs = Array.from(
    { length: SAMPLES },
    (_, i) => min + ((max - min) * i) / (SAMPLES - 1)
  )
  const budget = createBudget(200_000),
    invalid: string[] = []
  const series = expressions.map((expr) => {
    try {
      const formula = parseExpression(expr.replace(/^y\s*=\s*/, ""))
      return {
        name: expr,
        data: xs.map((x) => {
          try {
            const value = formula.evaluate({
              lookup: (name) => (name === "x" ? x : undefined),
              budget,
            })
            return typeof value === "number" && Number.isFinite(value)
              ? value
              : null
          } catch {
            return null
          }
        }),
      }
    } catch {
      invalid.push(expr)
      return { name: expr, data: [] }
    }
  })
  return { xs, series, invalid }
}

type PlotToolbarInput = {
  expressions: string[]
  initial: number[]
  viewport: number[]
  changeExpressions: (values: string[]) => void
  changeViewport: (values: number[]) => void
}
function PlotToolbar({
  expressions,
  initial,
  viewport: [min, max],
  changeExpressions,
  changeViewport,
}: PlotToolbarInput) {
  const t = useTranslations("IntelligentUI")
  const center = (min + max) / 2,
    span = max - min
  const actions = [
    {
      label: t("addExpression"),
      disabled: expressions.length >= MAX_FUNCTIONS,
      run: () => changeExpressions([...expressions, "x"]),
    },
    {
      label: t("zoomIn"),
      run: () => changeViewport([center - span / 4, center + span / 4]),
    },
    {
      label: t("zoomOut"),
      run: () => changeViewport([center - span, center + span]),
    },
    {
      label: t("previous"),
      run: () => changeViewport([min - span / 2, max - span / 2]),
    },
    {
      label: t("next"),
      run: () => changeViewport([min + span / 2, max + span / 2]),
    },
    { label: t("reset"), run: () => changeViewport(initial) },
  ]
  return (
    <div className="flex flex-wrap items-center gap-1">
      {actions.map((action) => (
        <Button
          key={action.label}
          size="sm"
          variant="outline"
          disabled={action.disabled}
          onClick={action.run}
        >
          {action.label}
        </Button>
      ))}
    </div>
  )
}
