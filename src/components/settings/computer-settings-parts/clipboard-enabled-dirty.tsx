"use client"

import { Values } from "../computer-settings"

export function clipboardEnabledDirty(
  values: Values,
  baseline: Values
): boolean {
  return values.clipboardEnabled !== baseline.clipboardEnabled
}
