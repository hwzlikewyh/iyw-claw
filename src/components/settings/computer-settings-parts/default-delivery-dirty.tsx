"use client"

import { Values } from "../computer-settings"

export function defaultDeliveryDirty(
  values: Values,
  baseline: Values
): boolean {
  return values.defaultDelivery !== baseline.defaultDelivery
}
