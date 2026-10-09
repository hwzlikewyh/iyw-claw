"use client"

import { Values } from "../computer-settings"

export function screenEnabledDirty(values: Values, baseline: Values): boolean {
  return values.screenEnabled !== baseline.screenEnabled
}
