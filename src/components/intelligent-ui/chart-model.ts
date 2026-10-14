import { list, record, text, numeric } from "./context"

export const CHART_WIDTH = 600,
  CHART_HEIGHT = 300,
  CHART_PAD = 42
const MAX_POINTS = 1000
export const chartColor = (index: number) =>
  `var(--chart-${(index % 5) + 1}, var(--primary))`
export interface ChartSeries {
  name: string
  index: number
  data: (number | null)[]
  base: number[]
}
export interface ChartModel {
  type: string
  xs: unknown[]
  series: ChartSeries[]
  count: number
  min: number
  max: number
  stacked: boolean
  unit: string
  numericX: boolean
}
export function chartModel(
  props: Record<string, unknown>,
  hidden: number[]
): ChartModel {
  const xs = list(props.x).slice(0, MAX_POINTS)
  const series = readSeries(props.series, hidden)
  const count = Math.min(
    MAX_POINTS,
    Math.max(xs.length, ...series.map((s) => s.data.length), 1)
  )
  const supportsStack = ["bar", "horizontal-bar", "area"].includes(
    String(props.type)
  )
  const stacked =
    supportsStack && (props.stacked === true || props.percent === true)
  applyStack({
    series,
    count,
    stacked,
    percent: supportsStack && props.percent === true,
  })
  const bounds = series.flatMap((s) =>
    s.data.flatMap((v, i) => (v === null ? [] : [s.base[i], s.base[i] + v]))
  )
  const min = numeric(list(props.y)[0], Math.min(0, ...bounds))
  const max = Math.max(
    min + 1e-9,
    numeric(list(props.y)[1], Math.max(min + 1, ...bounds))
  )
  return {
    type: typeof props.type === "string" ? props.type : "line",
    xs,
    series,
    count,
    min,
    max,
    stacked,
    unit: text(props.unit ?? (props.percent ? "%" : "")),
    numericX:
      xs.length > 1 &&
      xs.every((x) => typeof x === "number" && Number.isFinite(x)),
  }
}
function applyStack({
  series,
  count,
  stacked,
  percent,
}: {
  series: ChartSeries[]
  count: number
  stacked: boolean
  percent: boolean
}) {
  for (let i = 0; i < count; i++) {
    const total =
      series.reduce((sum, s) => sum + Math.abs(s.data[i] ?? 0), 0) || 1
    let positive = 0,
      negative = 0
    for (const s of series) {
      const v = s.data[i]
      if (v === null || v === undefined) {
        s.base[i] = 0
        continue
      }
      const value = percent ? (v * 100) / total : v
      s.data[i] = value
      s.base[i] = stacked ? (value >= 0 ? positive : negative) : 0
      if (value >= 0) positive += value
      else negative += value
    }
  }
}
export function chartScale(model: ChartModel) {
  const { min, max, xs, numericX, count } = model
  const numeric = xs as number[]
  const low = numericX ? Math.min(...numeric) : 0
  const high = numericX
    ? Math.max(low + 1e-9, ...numeric)
    : Math.max(1, count - 1)
  return {
    x: (i: number) =>
      CHART_PAD +
      (((numericX
        ? (numeric[i] ?? low + ((high - low) * i) / Math.max(1, count - 1))
        : i) -
        low) /
        (high - low)) *
        (CHART_WIDTH - CHART_PAD * 2),
    y: (v: number) =>
      CHART_HEIGHT -
      CHART_PAD -
      ((v - min) / (max - min)) * (CHART_HEIGHT - CHART_PAD * 2),
  }
}

function readSeries(raw: unknown, hidden: number[]) {
  const series = list(raw)
    .slice(0, 12)
    .map((value, index) => {
      const s = record(value)
      return {
        name: text(s.name ?? index + 1),
        index,
        data: list(s.data)
          .slice(0, MAX_POINTS)
          .map((v) => (typeof v === "number" && Number.isFinite(v) ? v : null)),
        base: [] as number[],
      }
    })
    .filter((series) => !hidden.includes(series.index))
  return series
}
