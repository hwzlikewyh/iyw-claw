"use client"

import { type ComputerToolsSettings } from "@/lib/computer/types"
import { Values } from "../computer-settings"

export function fromSettings(settings: ComputerToolsSettings): Values {
  return {
    ttl: settings.grantTtlMinutes,
    blocklist: settings.blocklist,
    removed: settings.blocklistRemoved,
    stopShortcut: settings.stopShortcut,
    showIndicator: settings.showIndicator,
    allowForeground: settings.allowForeground,
    defaultDelivery: settings.defaultDelivery,
    launchEnabled: settings.launchEnabled ?? false,
    clipboardEnabled: settings.clipboardEnabled ?? false,
    screenEnabled: settings.screenEnabled ?? false,
  }
}
