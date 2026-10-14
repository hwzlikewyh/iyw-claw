import { text } from "./context"
import { chartColor, type ChartModel } from "./chart-model"

const RADIUS = 104,
  CENTER_X = 300,
  CENTER_Y = 145
export function SpecialChart({ model }: { model: ChartModel }) {
  if (model.type === "radar") return <Radar model={model} />
  if (model.type === "funnel") return <Funnel model={model} />
  if (model.type === "radial") return <Radial model={model} />
  return <Pie model={model} />
}
function Pie({ model }: { model: ChartModel }) {
  const values =
    model.series[0]?.data.slice(0, 100).map((v) => Math.max(0, v ?? 0)) ?? []
  const total = values.reduce((sum, v) => sum + v, 0) || 1
  return (
    <g transform={`translate(${CENTER_X},${CENTER_Y})`}>
      {values.map((value, index) => {
        const start =
          (values.slice(0, index).reduce((sum, v) => sum + v, 0) *
            2 *
            Math.PI) /
          total
        const end = start + (value * 2 * Math.PI) / total
        const label = `${text(model.xs[index] ?? index)}: ${value}${model.unit}`
        if (value === total)
          return (
            <circle key={index} r={RADIUS} fill={chartColor(index)}>
              <title>{label}</title>
            </circle>
          )
        const d = `M 0 0 L ${Math.cos(start) * RADIUS} ${Math.sin(start) * RADIUS} A ${RADIUS} ${RADIUS} 0 ${end - start > Math.PI ? 1 : 0} 1 ${Math.cos(end) * RADIUS} ${Math.sin(end) * RADIUS} Z`
        return (
          <path key={index} d={d} fill={chartColor(index)}>
            <title>{label}</title>
          </path>
        )
      })}
    </g>
  )
}
function Radar({ model }: { model: ChartModel }) {
  const count = Math.max(3, Math.min(model.count, 40))
  const point = (i: number, value: number) => {
    const angle = (i / count) * Math.PI * 2 - Math.PI / 2
    return `${CENTER_X + Math.cos(angle) * value},${CENTER_Y + Math.sin(angle) * value}`
  }
  return (
    <g>
      {[0.25, 0.5, 0.75, 1].map((fraction) => (
        <polygon
          key={fraction}
          points={Array.from({ length: count }, (_, i) =>
            point(i, RADIUS * fraction)
          ).join(" ")}
          fill="none"
          stroke="var(--border)"
        />
      ))}
      {model.series.map((s) => (
        <polygon
          key={s.index}
          points={Array.from({ length: count }, (_, i) =>
            point(
              i,
              (Math.max(0, s.data[i] ?? 0) / Math.max(1, model.max)) * RADIUS
            )
          ).join(" ")}
          fill={chartColor(s.index)}
          fillOpacity={0.15}
          stroke={chartColor(s.index)}
        >
          <title>
            {s.name}: {s.data.join(", ")}
          </title>
        </polygon>
      ))}
      <RadarLabels model={model} count={count} point={point} />
    </g>
  )
}
function Funnel({ model }: { model: ChartModel }) {
  const data = (model.series[0]?.data ?? [])
    .slice(0, 20)
    .map((v) => Math.max(0, v ?? 0))
  const max = Math.max(1, ...data),
    row = 230 / Math.max(1, data.length)
  return (
    <g>
      {data.map((value, i) => {
        const top = (value / max) * 400,
          bottom = ((data[i + 1] ?? value) / max) * 400,
          y = 25 + i * row
        return (
          <g key={i}>
            <polygon
              points={`${300 - top / 2},${y} ${300 + top / 2},${y} ${300 + bottom / 2},${y + row - 2} ${300 - bottom / 2},${y + row - 2}`}
              fill={chartColor(i)}
            >
              <title>
                {text(model.xs[i])}: {value}
              </title>
            </polygon>
            <text
              x={300}
              y={y + row / 2 + 3}
              textAnchor="middle"
              fontSize={11}
              fill="var(--foreground)"
            >
              {text(model.xs[i] ?? "")} · {value}
              {model.unit}
            </text>
          </g>
        )
      })}
    </g>
  )
}
function Radial({ model }: { model: ChartModel }) {
  const values = (model.series[0]?.data ?? []).slice(0, 8)
  const max = Math.max(1, model.max)
  return (
    <g transform={`translate(${CENTER_X},${CENTER_Y})`}>
      {values.map((v, i) => {
        const radius = RADIUS - i * 12,
          length = Math.PI * 2 * radius
        return (
          <g key={i}>
            <circle
              r={radius}
              fill="none"
              stroke="var(--muted)"
              strokeWidth={9}
            />
            <circle
              r={radius}
              fill="none"
              stroke={chartColor(i)}
              strokeWidth={9}
              strokeDasharray={`${(Math.max(0, v ?? 0) / max) * length} ${length}`}
              transform="rotate(-90)"
            >
              <title>
                {text(model.xs[i] ?? i)}: {v}
                {model.unit}
              </title>
            </circle>
          </g>
        )
      })}
    </g>
  )
}

function RadarLabels({
  model,
  count,
  point,
}: {
  model: ChartModel
  count: number
  point: (i: number, radius: number) => string
}) {
  return (
    <g>
      {Array.from({ length: count }, (_, i) => {
        const [x, y] = point(i, RADIUS + 20).split(",")
        return (
          <text
            key={i}
            x={x}
            y={y}
            textAnchor="middle"
            fontSize={10}
            fill="var(--muted-foreground)"
          >
            {text(model.xs[i] ?? i + 1)}
          </text>
        )
      })}
    </g>
  )
}
