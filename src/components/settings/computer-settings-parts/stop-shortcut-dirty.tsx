"use client"

import { Values } from "../computer-settings"

export function stopShortcutDirty(values: Values, baseline: Values): boolean {
  return values.stopShortcut !== baseline.stopShortcut
}
