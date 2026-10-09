"use client"

import { Values } from "../computer-settings"

export function ttlDirty(values: Values, baseline: Values): boolean {
  return values.ttl !== baseline.ttl
}
