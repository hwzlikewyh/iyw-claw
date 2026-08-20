"use client"

import { useMemo, useState } from "react"
import { BrainCircuit } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Slider } from "@/components/ui/slider"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { SessionConfigValueIcon } from "@/components/chat/session-config-value-icon"
import { cn } from "@/lib/utils"
import type { SessionConfigOptionInfo } from "@/lib/types"

/** The canonical effort-level order used to map option values → slider index. */
const EFFORT_ORDER: string[] = [
  "none",
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]

function normalizedValue(v: string): string {
  return v.trim().toLowerCase().replace(/_/g, "-")
}

/** Map "extra-high" → "xhigh" so the order lookup always finds a match. */
function canonicalize(v: string): string {
  const n = normalizedValue(v)
  if (n === "extra-high" || n === "extra_high") return "xhigh"
  return n
}

interface ReasoningEffortSliderProps {
  option: SessionConfigOptionInfo
  onSelect: (configId: string, valueId: string) => void
}

export function ReasoningEffortSlider({
  option,
  onSelect,
}: ReasoningEffortSliderProps) {
  const [open, setOpen] = useState(false)

  // Build a stable ordered list from the server-supplied options, respecting
  // the canonical effort order. Options the server doesn't send are omitted.
  const orderedOptions = useMemo(() => {
    if (option.kind.type !== "select") return []
    const byCanon = new Map(
      option.kind.options.map((o) => [canonicalize(o.value), o])
    )
    const ordered = EFFORT_ORDER.flatMap((key) => {
      const o = byCanon.get(key)
      return o ? [o] : []
    })
    // Append any server options not in the canonical list (future-proof).
    const known = new Set(EFFORT_ORDER)
    for (const o of option.kind.options) {
      if (!known.has(canonicalize(o.value))) ordered.push(o)
    }
    return ordered
  }, [option.kind])

  const currentIndex = useMemo(() => {
    if (option.kind.type !== "select") return 0
    const canon = canonicalize(option.kind.current_value)
    const idx = orderedOptions.findIndex((o) => canonicalize(o.value) === canon)
    return idx >= 0 ? idx : 0
  }, [option.kind, orderedOptions])

  if (option.kind.type !== "select" || orderedOptions.length === 0) return null

  const currentOption = orderedOptions[currentIndex]
  const max = orderedOptions.length - 1

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          title={option.name}
          aria-label={
            currentOption
              ? `${option.name}: ${currentOption.name}`
              : option.name
          }
          className="min-w-0 gap-1 px-1.5 text-muted-foreground"
        >
          <span className="flex size-3.5 shrink-0 items-center justify-center">
            <SessionConfigValueIcon
              configId={option.id}
              value={option.kind.current_value}
              className="size-3.5"
            />
          </span>
          <span className="max-w-[5rem] truncate text-xs">
            {currentOption?.name ?? option.kind.current_value}
          </span>
        </Button>
      </PopoverTrigger>

      <PopoverContent
        side="top"
        align="start"
        className="w-72 p-3"
        sideOffset={6}
      >
        {/* Header */}
        <div className="mb-3 flex items-center gap-1.5">
          <BrainCircuit className="size-3.5 shrink-0 text-muted-foreground" />
          <span className="text-xs font-medium text-foreground">
            {option.name}
          </span>
        </div>

        {/* Slider track */}
        <div className="px-1">
          <Slider
            min={0}
            max={max}
            step={1}
            value={[currentIndex]}
            onValueChange={([idx]) => {
              const picked = orderedOptions[idx]
              if (picked) onSelect(option.id, picked.value)
            }}
            onValueCommit={() => setOpen(false)}
            aria-label={option.name}
          />
        </div>

        {/* Tick labels — clickable for keyboard/mouse users */}
        <div className="mt-1.5 flex justify-between px-0.5" aria-hidden="true">
          {orderedOptions.map((o, i) => (
            <button
              key={o.value}
              type="button"
              onClick={() => {
                onSelect(option.id, o.value)
                setOpen(false)
              }}
              className={cn(
                "flex-1 text-center text-[10px] leading-tight transition-colors first:text-left last:text-right",
                i === currentIndex
                  ? "font-semibold text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              )}
            >
              {o.name}
            </button>
          ))}
        </div>

        {/* Current level description */}
        {currentOption?.description && (
          <p className="mt-2.5 border-t border-border/50 pt-2 text-xs text-muted-foreground">
            {currentOption.description}
          </p>
        )}
      </PopoverContent>
    </Popover>
  )
}
