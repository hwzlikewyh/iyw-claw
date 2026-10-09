"use client"

import { Values } from "../computer-settings"

export function removedDirty(values: Values, baseline: Values): boolean {
  return (
    [...values.removed].sort().join("\n") !==
    [...baseline.removed].sort().join("\n")
  )
}
