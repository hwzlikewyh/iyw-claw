"use client"

import { Zap } from "lucide-react"
import { isFastConfigOption } from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"
import { Switch } from "@/components/ui/switch"

export function ModelFastModeToggle({
  options,
  onSelect,
}: {
  options: SessionConfigOptionInfo[]
  onSelect?: (configId: string, valueId: string) => void
}) {
  const option = options.find(isFastConfigOption)
  if (!option || !onSelect) return null
  if (
    !["on", "off"].every((value) =>
      option.kind.options.some((item) => item.value === value)
    )
  )
    return null
  const enabled = option.kind.current_value === "on"
  return (
    <div className="flex w-full items-center gap-2 border-b border-primary/10 px-3 py-3 text-sm">
      <Zap className="size-4 text-muted-foreground" />
      <span className="flex-1 text-left">{option.name}</span>
      <Switch
        aria-label={option.name}
        checked={enabled}
        onCheckedChange={(checked) =>
          onSelect(option.id, checked ? "on" : "off")
        }
      />
    </div>
  )
}
