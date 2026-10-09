"use client"

import { Values } from "../computer-settings"

export function launchEnabledDirty(values: Values, baseline: Values): boolean {
  return values.launchEnabled !== baseline.launchEnabled
}
