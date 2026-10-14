import { text } from "./context"
import {
  chartColor,
  chartScale,
  CHART_HEIGHT as H,
  CHART_WIDTH as W,
  CHART_PAD as P,
  type ChartModel,
  type ChartSeries,
} from "./chart-model"

export function CartesianChart({ model }: { model: ChartModel }) {
  if (model.type === "horizontal-bar") return <HorizontalBars model={model} />
  const scale = chartScale(model)
  return (
    <g>
      <Axes model={model} />
      {model.series.map((s, i) => (
        <CartesianSeries
          key={s.index}
          model={model}
          series={s}
          slot={i}
          scale={scale}
        />
      ))}
    </g>
  )
}
function Axes({ model }: { model: ChartModel }) {
  return (
    <g fill="var(--muted-foreground)" fontSize={10}>
      {Array.from({ length: 5 }, (_, i) => {
        const value = model.min + ((model.max - model.min) * i) / 4
        const y = chartScale(model).y(value)
        return (
          <g key={i}>
            <line
              x1={P}
              x2={W - P}
              y1={y}
              y2={y}
              stroke="var(--border)"
              opacity={0.5}
            />
            <text x={P - 4} y={y + 3} textAnchor="end">
              {Number(value.toPrecision(3))}
            </text>
          </g>
        )
      })}
      <text x={P} y={H - 12}>
        {text(model.xs[0] ?? "")}
      </text>
      <text x={W - P} y={H - 12} textAnchor="end">
        {text(model.xs[model.xs.length - 1] ?? "")}
      </text>
      <text x={P} y={14}>
        {model.unit}
      </text>
    </g>
  )
}
function CartesianSeries({
  model,
  series: s,
  slot,
  scale,
}: {
  model: ChartModel
  series: ChartSeries
  slot: number
  scale: ReturnType<typeof chartScale>
}) {
  if (model.type === "bar")
    return <SeriesBars model={model} series={s} slot={slot} scale={scale} />
  const color = chartColor(s.index)
  return (
    <g>
      <LineAreas model={model} series={s} scale={scale} color={color} />
      {s.data.map((v, i) =>
        v === null ? null : (
          <circle
            key={i}
            cx={scale.x(i)}
            cy={scale.y(s.base[i] + v)}
            r={model.type === "scatter" ? 4 : 2}
            fill={color}
          >
            <title>
              {text(model.xs[i] ?? i)} · {s.name}: {v}
              {model.unit}
            </title>
          </circle>
        )
      )}
    </g>
  )
}
function lineSegments(data: (number | null)[]) {
  const segments: number[][] = []
  data.forEach((value, i) => {
    if (value === null) return
    if (i === 0 || data[i - 1] === null) segments.push([])
    segments[segments.length - 1].push(i)
  })
  return segments
}
function HorizontalBars({ model }: { model: ChartModel }) {
  const x = (v: number) =>
    P + ((v - model.min) / (model.max - model.min)) * (W - P * 2)
  const row = (H - P * 2) / model.count
  return (
    <g>
      {model.series.map((s, slot) => (
        <g key={s.index}>
          {s.data.map((v, i) =>
            v === null ? null : (
              <g key={i}>
                <rect
                  x={Math.min(x(s.base[i]), x(s.base[i] + v))}
                  y={
                    P +
                    i * row +
                    (model.stacked
                      ? 0
                      : (slot * row * 0.7) / model.series.length)
                  }
                  width={Math.abs(x(s.base[i] + v) - x(s.base[i]))}
                  height={Math.max(
                    0.5,
                    (row * 0.7) / (model.stacked ? 1 : model.series.length)
                  )}
                  fill={chartColor(s.index)}
                >
                  <title>
                    {text(model.xs[i] ?? i)} · {s.name}: {v}
                    {model.unit}
                  </title>
                </rect>
                {slot === 0 && <BarLabel model={model} index={i} row={row} />}
              </g>
            )
          )}
        </g>
      ))}
    </g>
  )
}

type SeriesInput = {
  model: ChartModel
  series: ChartSeries
  slot: number
  scale: ReturnType<typeof chartScale>
}
function SeriesBars({ model, series: s, slot, scale }: SeriesInput) {
  const color = chartColor(s.index)
  const width = Math.min(40, ((W - P * 2) / model.count) * 0.7)
  return (
    <g>
      {s.data.map((v, i) =>
        v === null ? null : (
          <rect
            key={i}
            x={
              P +
              ((i + 0.5) * (W - P * 2)) / model.count -
              width / 2 +
              (model.stacked ? 0 : (slot * width) / model.series.length)
            }
            y={Math.min(scale.y(s.base[i]), scale.y(s.base[i] + v))}
            width={model.stacked ? width : width / model.series.length}
            height={Math.max(
              0.5,
              Math.abs(scale.y(s.base[i]) - scale.y(s.base[i] + v))
            )}
            fill={color}
          >
            <title>
              {text(model.xs[i] ?? i)} · {s.name}: {v}
              {model.unit}
            </title>
          </rect>
        )
      )}
    </g>
  )
}
function LineAreas({
  model,
  series: s,
  scale,
  color,
}: Omit<SeriesInput, "slot"> & { color: string }) {
  const segments = lineSegments(s.data)
  return (
    <g>
      {segments.map((indices, i) => {
        const points = indices
          .map((j) => `${scale.x(j)},${scale.y(s.base[j] + s.data[j]!)}`)
          .join(" ")
        const floor = [...indices]
          .reverse()
          .map((j) => `${scale.x(j)},${scale.y(s.base[j])}`)
          .join(" ")
        return (
          <g key={i}>
            {model.type === "area" && (
              <polygon
                points={`${points} ${floor}`}
                fill={color}
                opacity={0.25}
              />
            )}
            {model.type !== "scatter" && (
              <polyline
                points={points}
                fill="none"
                stroke={color}
                strokeWidth={2}
              />
            )}
          </g>
        )
      })}
    </g>
  )
}

function BarLabel({
  model,
  index: i,
  row,
}: {
  model: ChartModel
  index: number
  row: number
}) {
  return (
    <text
      x={P - 3}
      y={P + i * row + row / 2}
      textAnchor="end"
      fill="var(--muted-foreground)"
      fontSize={10}
    >
      {text(model.xs[i] ?? i)}
    </text>
  )
}
