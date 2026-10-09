import { useEffect, useSyncExternalStore } from "react"
import { isLocalDesktop, onTransportReconnect } from "@/lib/platform"
import { getTransport, onTransportChange } from "@/lib/transport"
import type {
  ComputerDelivery,
  ComputerStatePayload,
  ComputerStatus,
  ComputerToolsSettings,
  DriverInfo,
  OsPermission,
  PermissionRequestResult,
  PickerWindow,
  ShareManyResult,
  SharedWindow,
  GrantLevel,
  StopKeyStatus,
} from "./types"
export async function getComputerToolsSettings(): Promise<ComputerToolsSettings> {
  return getTransport().call("get_computer_tools_settings")
}
export async function setComputerToolsEnabled(
  enabled: boolean
): Promise<ComputerToolsSettings> {
  return getTransport().call("set_computer_tools_enabled", { enabled })
}
export async function setComputerToolsPreferences(preferences: {
  grantTtlMinutes?: number
  blocklist?: string[]
  blocklistRemoved?: string[]
  stopShortcut?: string
  showIndicator?: boolean
  allowForeground?: boolean
  defaultDelivery?: ComputerDelivery
  launchEnabled?: boolean
  clipboardEnabled?: boolean
  screenEnabled?: boolean
}): Promise<ComputerToolsSettings> {
  return getTransport().call("set_computer_tools_preferences", preferences)
}
export interface ComputerServed {
  available: boolean
  platform: "macos" | "windows" | "linux"
}
let served: ComputerServed | null = null
let asking: Promise<boolean> | null = null
let askAgain = false
let reconnectsWatched = false
let transportGeneration = 0
let unwatchReconnect: (() => void) | null = null
const servedListeners = new Set<() => void>()
export function computerAvailable(): boolean {
  return served?.available === true
}
export function computerServerPlatform(): ComputerServed["platform"] | null {
  return isLocalDesktop() ? null : (served?.platform ?? null)
}
export function askComputerServed(): Promise<boolean> {
  if (served !== null) return Promise.resolve(served.available)
  return ask()
}
function ask(): Promise<boolean> {
  watchReconnects()
  if (asking) return asking
  const generation = transportGeneration
  asking = getTransport()
    .call<ComputerServed>("computer_available", {})
    .then(
      (answer) => {
        if (generation !== transportGeneration) return false
        const changed =
          served?.available !== answer.available ||
          served?.platform !== answer.platform
        served = answer
        if (changed) for (const listener of servedListeners) listener()
        return answer.available
      },
      () => served?.available ?? false
    )
    .finally(() => {
      if (generation !== transportGeneration) return
      asking = null
      if (askAgain) {
        askAgain = false
        void ask()
      }
    })
  return asking
}
function watchReconnects() {
  if (reconnectsWatched) return
  reconnectsWatched = true
  unwatchReconnect = onTransportReconnect(() => {
    if (asking) askAgain = true
    else void ask()
  })
}

onTransportChange(() => {
  transportGeneration += 1
  served = null
  asking = null
  askAgain = false
  unwatchReconnect?.()
  reconnectsWatched = false
  for (const listener of servedListeners) listener()
  void ask()
})
export function subscribeComputerServed(listener: () => void): () => void {
  servedListeners.add(listener)
  return () => servedListeners.delete(listener)
}
export function useComputerAvailable(): boolean {
  const available = useSyncExternalStore(
    subscribeComputerServed,
    computerAvailable,
    computerAvailable
  )
  useEffect(() => {
    if (!available) void askComputerServed()
  }, [available])
  return available
}
export function resetComputerServedForTest(
  answer: ComputerServed | null = null
) {
  served = answer
  asking = null
  askAgain = false
  reconnectsWatched = false
}
export async function computerStatus(): Promise<ComputerStatus> {
  return getTransport().call("computer_status", {})
}
export async function computerSharedState(): Promise<ComputerStatePayload> {
  return getTransport().call("computer_shared_state", {})
}
export async function computerRequestPermission(
  permission: OsPermission
): Promise<PermissionRequestResult> {
  return getTransport().call("computer_request_permission", { permission })
}
export async function computerOpenPermissionSettings(
  permission: OsPermission
): Promise<void> {
  return getTransport().call("computer_open_permission_settings", {
    permission,
  })
}
export async function computerRevealHelper(): Promise<void> {
  return getTransport().call("computer_reveal_helper", {})
}
export async function computerListShareableWindows(): Promise<PickerWindow[]> {
  return getTransport().call("computer_list_shareable_windows", {})
}
export async function computerWindowThumbnail(
  targetId: string
): Promise<string | null> {
  return getTransport().call("computer_window_thumbnail", { targetId })
}
export async function computerShareWindow(
  targetId: string,
  level: GrantLevel
): Promise<SharedWindow[]> {
  return getTransport().call("computer_share_window", { targetId, level })
}
export async function computerShareWindows(
  targetIds: string[],
  level: GrantLevel
): Promise<ShareManyResult> {
  return getTransport().call("computer_share_windows", { targetIds, level })
}
export async function computerShareApp(
  app:
    | {
        targetId: string
      }
    | {
        appId: string
      },
  level: GrantLevel
): Promise<ComputerStatePayload> {
  return getTransport().call("computer_share_app", {
    targetId: "targetId" in app ? app.targetId : null,
    appId: "appId" in app ? app.appId : null,
    level,
  })
}
export async function computerShareScreen(
  level: GrantLevel
): Promise<ComputerStatePayload> {
  return getTransport().call("computer_share_screen", { level })
}
export async function computerRevokeAll(): Promise<void> {
  return getTransport().call("computer_revoke_all", {})
}
export async function computerStop(): Promise<void> {
  return getTransport().call("computer_stop", {})
}
export async function computerStopKeyStatus(): Promise<StopKeyStatus> {
  return getTransport().call("computer_stop_key_status", {})
}
export async function computerIndicatorFit(
  width: number,
  height: number
): Promise<void> {
  return getTransport().call("computer_indicator_fit", { width, height })
}
export async function computerDriverInfo(): Promise<DriverInfo> {
  return getTransport().call("computer_driver_info", {})
}
export async function computerDriverInstall(): Promise<DriverInfo> {
  return getTransport().call(
    "computer_driver_install",
    {},
    { timeoutMs: 600000 }
  )
}
export async function computerDriverUninstall(): Promise<DriverInfo> {
  return getTransport().call("computer_driver_uninstall", {})
}
