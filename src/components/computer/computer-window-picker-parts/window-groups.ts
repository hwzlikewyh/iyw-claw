import type {
  GrantLevel,
  PickerWindow,
  SharedWindow,
} from "@/lib/computer/types"
import type { AppGroup } from "../computer-window-picker"

export function windowShareState(
  window: PickerWindow,
  sharing: { known: boolean; windows: readonly SharedWindow[] }
) {
  const current = sharing.known
    ? sharing.windows.find((item) => item.targetId === window.targetId)
    : window
  return {
    level: current?.level ?? ("none" as GrantLevel),
    wholeApp: !!current?.wholeApp,
    appId: current?.appId,
    wholeScreen: !!current?.wholeScreen,
  }
}

export function groupWindows(windows: PickerWindow[], unnamedApp: string) {
  const groups: AppGroup[] = []
  for (const window of windows) {
    const key = `${window.pid}:${window.appKey}`
    let group = groups.find((item) => item.key === key)
    if (!group) {
      group = { key, appName: window.appName || unnamedApp, windows: [] }
      groups.push(group)
    }
    group.windows.push(window)
  }
  return groups
}
