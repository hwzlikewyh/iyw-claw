"use client"

import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import type { ResolvedNode } from "@/lib/intelligent-ui/engine/block"
import { useUiRender, numeric, text, list, record } from "./context"

type InputProps = { node: ResolvedNode; value: unknown; id: string }
export function ControlField({ node, value, id }: InputProps) {
  const { update } = useUiRender(),
    p = node.props,
    bind = node.bind!
  if (node.component === "switch")
    return (
      <input
        id={id}
        type="checkbox"
        checked={value === true}
        onChange={(event) => update({ [bind]: event.target.checked })}
      />
    )
  if (node.component === "slider") {
    const min = numeric(p.min),
      max = Math.max(min, numeric(p.max, 100)),
      step = Math.max(Number.EPSILON, numeric(p.step, 1))
    return (
      <input
        id={id}
        className="w-full accent-primary"
        type="range"
        min={min}
        max={max}
        step={step}
        value={numeric(value)}
        onChange={(event) => update({ [bind]: Number(event.target.value) })}
      />
    )
  }
  if (node.component === "input")
    return <TextInput node={node} value={value} id={id} />
  return <SelectControl node={node} value={value} id={id} />
}
function TextInput({ node, value, id }: InputProps) {
  const { update } = useUiRender(),
    p = node.props
  const attributes = {
    id,
    type: p.type === "number" ? "number" : "text",
    value: typeof value === "string" || typeof value === "number" ? value : "",
    maxLength: 200,
    placeholder: typeof p.placeholder === "string" ? p.placeholder : undefined,
  }
  return (
    <Input
      {...attributes}
      onChange={(event) =>
        update({
          [node.bind!]:
            p.type === "number"
              ? Number.isFinite(event.target.valueAsNumber)
                ? event.target.valueAsNumber
                : null
              : event.target.value,
        })
      }
    />
  )
}
export function SelectControl({ node, value, id }: InputProps) {
  const { update } = useUiRender()
  const options = list(node.props.options)
    .slice(0, 100)
    .map((item) =>
      typeof item === "string" ? { label: item, value: item } : record(item)
    )
  const selected = options.findIndex((item) => item.value === value)
  const change = (index: number) => {
    const v = options[index]?.value
    if (
      typeof v === "string" ||
      typeof v === "number" ||
      typeof v === "boolean"
    )
      update({ [node.bind!]: v })
  }
  if (node.component === "segmented")
    return (
      <Segmented
        id={id}
        label={text(node.props.label ?? node.bind)}
        options={options}
        selected={selected}
        change={change}
      />
    )
  return (
    <select
      id={id}
      className="h-9 w-full rounded-md border bg-background px-2"
      value={selected}
      onChange={(e) => change(Number(e.target.value))}
    >
      {selected < 0 && (
        <option value={-1} disabled>
          —
        </option>
      )}
      {options.map((option, i) => (
        <option value={i} key={i}>
          {text(option.label)}
        </option>
      ))}
    </select>
  )
}

function Segmented({
  id,
  label,
  options,
  selected,
  change,
}: {
  id: string
  label: string
  options: Record<string, unknown>[]
  selected: number
  change: (index: number) => void
}) {
  return (
    <div
      id={id}
      role="group"
      aria-label={label}
      className="flex flex-wrap gap-1"
    >
      {options.map((option, i) => (
        <Button
          key={i}
          size="sm"
          variant={selected === i ? "default" : "outline"}
          aria-pressed={selected === i}
          onClick={() => change(i)}
        >
          {text(option.label)}
        </Button>
      ))}
    </div>
  )
}
