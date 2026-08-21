"use client"

import { Circle, Gauge, Zap } from "lucide-react"
import { cn } from "@/lib/utils"

interface SessionConfigValueIconProps {
  configId: string
  value: string
  className?: string
}

function normalized(value: string): string {
  return value.trim().toLowerCase().replace(/_/g, "-")
}

function isReasoningConfig(configId: string): boolean {
  const id = normalized(configId)
  return (
    id.includes("reasoning") || id.includes("thought") || id.includes("effort")
  )
}

export function hasSessionConfigValueIcon(configId: string): boolean {
  const id = normalized(configId)
  return id === "fast" || id === "fast-mode" || isReasoningConfig(id)
}

export function SessionConfigValueIcon({
  configId,
  value,
  className,
}: SessionConfigValueIconProps) {
  const id = normalized(configId)
  const normalizedValue = normalized(value)

  if (id === "fast" || id === "fast-mode") {
    return normalizedValue === "on" || normalizedValue === "fast" ? (
      <Zap
        aria-hidden="true"
        className={cn("size-4 text-primary", className)}
      />
    ) : (
      <Circle
        aria-hidden="true"
        className={cn("size-2 fill-current text-muted-foreground", className)}
      />
    )
  }

  if (!isReasoningConfig(id)) return null

  return (
    <Gauge
      aria-hidden="true"
      className={cn("size-4 text-muted-foreground", className)}
    />
  )
}
