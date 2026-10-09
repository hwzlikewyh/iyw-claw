"use client"

import { type ComputerDelivery } from "@/lib/computer/types"

export const TTL_CHOICES = [10, 30, 60, 240, 0] as const

export interface Values {
  ttl: number
  blocklist: string[]
  removed: string[]
  stopShortcut: string
  showIndicator: boolean
  allowForeground: boolean
  defaultDelivery: ComputerDelivery
  launchEnabled: boolean
  clipboardEnabled: boolean
  screenEnabled: boolean
}

export const EMPTY: Values = {
  ttl: 30,
  blocklist: [],
  removed: [],
  stopShortcut: "",
  showIndicator: true,
  allowForeground: true,
  defaultDelivery: "background",
  launchEnabled: false,
  clipboardEnabled: false,
  screenEnabled: false,
}

export { ComputerSettingsSection } from "./computer-settings-parts/computer-settings-section"
export const SYSTEM_ENTRY_NAMES = {
  "system-settings": "blocklist.items.systemSettings",
  "credential-prompts": "blocklist.items.credentialPrompts",
  keychain: "blocklist.items.keychain",
  passwords: "blocklist.items.passwords",
} as const
