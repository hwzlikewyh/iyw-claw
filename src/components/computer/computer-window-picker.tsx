"use client"

import type { PickerWindow } from "@/lib/computer/types"

export const GRID = "grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-3"

export interface AppGroup {
  key: string
  appName: string
  windows: PickerWindow[]
}

export const LEVELS = [
  { level: "none", label: "levelNone" },
  { level: "read", label: "levelRead" },
  { level: "control", label: "levelControl" },
] as const

export { ComputerWindowPicker } from "./computer-window-picker-parts/computer-window-picker"
