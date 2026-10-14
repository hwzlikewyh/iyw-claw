"use client"

import { formulaToLatex } from "@/lib/intelligent-ui/engine/expression"
import { MessageResponse } from "@/components/ai-elements/message"
import { list, record, text, numeric, useUiRender } from "./context"
import { useId } from "react"

const SCENE_DEFAULT_WIDTH = 360,
  SCENE_DEFAULT_HEIGHT = 220
export function Scene({ props }: { props: Record<string, unknown> }) {
  const width = Math.min(
    1000,
    Math.max(100, numeric(props.width, SCENE_DEFAULT_WIDTH))
  )
  const height = Math.min(
    640,
    Math.max(50, numeric(props.height, SCENE_DEFAULT_HEIGHT))
  )
  return (
    <svg
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={text(props.title ?? "Diagram")}
      className="max-h-96 w-full"
    >
      {list(props.shapes)
        .slice(0, 100)
        .map((shape, i) => (
          <Shape key={i} props={record(shape)} />
        ))}
    </svg>
  )
}
type ShapeProps = { props: Record<string, unknown>; color: string }
function Shape({ props: p }: { props: Record<string, unknown> }) {
  const color =
    p.tone === "negative"
      ? "var(--destructive)"
      : p.tone === "muted"
        ? "var(--muted-foreground)"
        : "var(--primary)"
  if (p.type === "rect") return <Rectangle props={p} color={color} />
  if (p.type === "circle") return <Circle props={p} color={color} />
  if (p.type === "line" || p.type === "arrow")
    return <Line props={p} color={color} />
  if (p.type === "text")
    return (
      <text x={numeric(p.x)} y={numeric(p.y)} fill={color} fontSize={12}>
        {text(p.text ?? p.label ?? "")}
      </text>
    )
  return null
}
function Rectangle({ props: p, color }: ShapeProps) {
  const attrs = {
    x: numeric(p.x),
    y: numeric(p.y),
    width: Math.max(0, numeric(p.w ?? p.width)),
    height: Math.max(0, numeric(p.h ?? p.height)),
    fill: color,
    opacity: 0.3,
  }
  return (
    <g>
      <rect {...attrs} />
      <text
        x={attrs.x + 4}
        y={attrs.y + 16}
        fill="var(--foreground)"
        fontSize={12}
      >
        {text(p.label ?? "")}
      </text>
    </g>
  )
}
function Circle({ props: p, color }: ShapeProps) {
  const attrs = {
    cx: numeric(p.cx),
    cy: numeric(p.cy),
    r: Math.max(0, numeric(p.r)),
    fill: color,
    opacity: 0.3,
  }
  return (
    <g>
      <circle {...attrs} />
      <text
        x={attrs.cx}
        y={attrs.cy}
        textAnchor="middle"
        fill="var(--foreground)"
        fontSize={12}
      >
        {text(p.label ?? "")}
      </text>
    </g>
  )
}
function Line({ props: p, color }: ShapeProps) {
  const markerId = useId()
  const attrs = {
    x1: numeric(p.x1),
    y1: numeric(p.y1),
    x2: numeric(p.x2),
    y2: numeric(p.y2),
    stroke: color,
    strokeWidth: 2,
    markerEnd: p.type === "arrow" ? `url(#${markerId})` : undefined,
  }
  return (
    <g>
      <line {...attrs} />
      {p.type === "arrow" && (
        <defs>
          <marker
            id={markerId}
            markerWidth={8}
            markerHeight={8}
            refX={6}
            refY={3}
            orient="auto"
          >
            <path d="M0,0 L0,6 L6,3 Z" fill={color} />
          </marker>
        </defs>
      )}
      <text
        x={(attrs.x1 + attrs.x2) / 2}
        y={(attrs.y1 + attrs.y2) / 2 - 6}
        fill="var(--foreground)"
        fontSize={12}
      >
        {text(p.label ?? "")}
      </text>
    </g>
  )
}
export function Formula({ props }: { props: Record<string, unknown> }) {
  const { values } = useUiRender()
  const expr = typeof props.expr === "string" ? props.expr : ""
  let markdown = ""
  try {
    const formula = formulaToLatex(expr, {
      values,
      with: props.with,
      name: typeof props.name === "string" ? props.name : undefined,
      unit: typeof props.unit === "string" ? props.unit : undefined,
      precision: numeric(props.precision, 2),
    })
    markdown =
      "$$" +
      formula.symbolic +
      "$$" +
      (formula.worked ? "\n\n$$" + formula.worked + "$$" : "")
  } catch {
    markdown = ""
  }
  return markdown ? (
    <div className="overflow-x-auto rounded-md bg-muted/40 p-3">
      <MessageResponse>{markdown}</MessageResponse>
    </div>
  ) : (
    <p className="rounded border p-3 text-xs">{expr} = —</p>
  )
}
