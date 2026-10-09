"use client"

import type { DriverInfo } from "@/lib/computer/types"

export function progressOf(info: DriverInfo | null): {
  value: number | null
  done: number | null
  total: number | null
} {
  const task = info?.task
  if (task?.kind !== "installing" || task.downloadedMb === undefined) {
    return { value: null, done: null, total: null }
  }
  const total = task.totalMb ?? null
  return {
    done: task.downloadedMb,
    total,
    value: total ? Math.min(100, (task.downloadedMb / total) * 100) : null,
  }
}
