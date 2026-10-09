"use client"

import { useComputerEnabled } from "@/lib/computer/use-computer-enabled"
import { ComputerPopover } from "./computer-popover"

export function StatusBarComputer() {
  const { enabled } = useComputerEnabled({ desktopOnly: true })
  if (!enabled) return null
  return <ComputerPopover />
}
