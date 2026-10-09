"use client"

import { Values } from "../computer-settings"

export function allowForegroundDirty(
  values: Values,
  baseline: Values
): boolean {
  return values.allowForeground !== baseline.allowForeground
}
