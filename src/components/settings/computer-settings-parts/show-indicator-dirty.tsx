"use client"

import { Values } from "../computer-settings"

export function showIndicatorDirty(values: Values, baseline: Values): boolean {
  return values.showIndicator !== baseline.showIndicator
}
