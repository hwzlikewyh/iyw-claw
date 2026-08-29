"use client"

import { useMemo, useState } from "react"
import { Check, ChevronRight, Gauge, Zap } from "lucide-react"
import { cn } from "@/lib/utils"
import {
  isFastConfigOption,
  isReasoningConfigOption,
} from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ModelBehaviorMenuProps {
  options: SessionConfigOptionInfo[]
  onSelect: (configId: string, valueId: string) => void
  compact?: boolean
}

function currentLabel(option: SessionConfigOptionInfo): string {
  return (
    option.kind.options.find(({ value }) => value === option.kind.current_value)
      ?.name ?? option.kind.current_value
  )
}

function BehaviorIcon({ option }: { option: SessionConfigOptionInfo }) {
  if (isFastConfigOption(option)) return <Zap className="size-4" />
  if (isReasoningConfigOption(option)) return <Gauge className="size-4" />
  return null
}

export function ModelBehaviorMenu({
  options,
  onSelect,
  compact = false,
}: ModelBehaviorMenuProps) {
  const visible = useMemo(
    () => options.filter((option) => option.kind.options.length > 1),
    [options]
  )
  const [activeId, setActiveId] = useState<string | null>(null)
  if (visible.length === 0) return null
  const active = visible.find(({ id }) => id === activeId) ?? null

  return (
    <div
      className={cn(
        "flex shrink-0 bg-popover",
        compact ? "w-full flex-col border-t" : "border-l"
      )}
    >
      <div
        className={cn("p-1", compact ? "w-full" : "w-44")}
        aria-label="模型行为"
      >
        {visible.map((option) => (
          <button
            key={option.id}
            type="button"
            onMouseEnter={() => setActiveId(option.id)}
            onFocus={() => setActiveId(option.id)}
            onClick={() => setActiveId(option.id)}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm",
              active?.id === option.id && "bg-accent text-accent-foreground"
            )}
          >
            <BehaviorIcon option={option} />
            <span className="min-w-0 flex-1 truncate">{option.name}</span>
            <span className="text-xs text-muted-foreground">
              {currentLabel(option)}
            </span>
            <ChevronRight className="size-3.5 shrink-0" />
          </button>
        ))}
      </div>
      {active ? (
        <div
          className={cn("p-1", compact ? "w-full border-t" : "w-52 border-l")}
          role="group"
          aria-label={active.name}
        >
          <div className="px-2 py-1.5 text-xs text-muted-foreground">
            {active.name}
          </div>
          {active.kind.options.map((value) => {
            const selected = value.value === active.kind.current_value
            return (
              <button
                key={value.value}
                type="button"
                aria-current={selected ? "true" : undefined}
                onClick={() => onSelect(active.id, value.value)}
                className={cn(
                  "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-sm",
                  "hover:bg-accent hover:text-accent-foreground",
                  selected && "bg-accent/60"
                )}
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{value.name}</span>
                  {value.description ? (
                    <span className="mt-0.5 block text-xs text-muted-foreground">
                      {value.description}
                    </span>
                  ) : null}
                </span>
                {selected ? <Check className="mt-0.5 size-4 shrink-0" /> : null}
              </button>
            )
          })}
        </div>
      ) : null}
    </div>
  )
}
