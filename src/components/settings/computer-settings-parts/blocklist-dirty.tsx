"use client"

import { Values } from "../computer-settings"

export function blocklistDirty(values: Values, baseline: Values): boolean {
  return values.blocklist.join("\n") !== baseline.blocklist.join("\n")
}
