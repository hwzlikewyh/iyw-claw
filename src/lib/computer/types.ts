export type GrantLevel = "none" | "read" | "control"
export type OsPermission = "accessibility" | "screenRecording"
export interface PermissionReport {
  required: boolean
  accessibility: boolean
  screenRecording: boolean
}
export interface PermissionRequestResult {
  report: PermissionReport
  prompted: boolean
}
export type BackendState =
  | "idle"
  | "downloading"
  | "starting"
  | "ready"
  | "failed"
export type PeerCheck = "verified" | "development" | "notApplicable"
export interface BackendStatus {
  state: BackendState
  detail?: string
  driverVersion: string
  peer?: PeerCheck
}
export interface HostTccStatus {
  accessibility: boolean
  screenRecording: boolean
  selfResponsible: boolean
}
export interface SharedWindow {
  targetId: string
  appName: string
  appKey: string
  title: string
  level: GrantLevel
  grantedAt: number
  lastUsedAt: number
  wholeApp?: boolean
  appId?: string
  wholeScreen?: boolean
}
export interface SharedScreen {
  level: GrantLevel
  grantedAt: number
  lastUsedAt: number
  windows: number
}
export interface SharedApp {
  appId: string
  appName: string
  appKey: string
  level: GrantLevel
  grantedAt: number
  lastUsedAt: number
  windows: number
}
export interface ComputerStatus {
  enabled: boolean
  platform: "macos" | "windows" | "linux"
  verifiedPlatform: boolean
  backend: BackendStatus
  permissions?: PermissionReport
  host?: HostTccStatus
  shared: SharedWindow[]
  screenOffered?: boolean
}
export interface Rect {
  x: number
  y: number
  width: number
  height: number
}
export type NotGrantable = "own-app" | "blocklisted" | "unidentified"
export interface PickerWindow {
  targetId: string
  appName: string
  appKey: string
  pid: number
  title: string
  bounds: Rect
  onScreen: boolean
  minimized: boolean
  hidden: boolean
  level: GrantLevel
  wholeApp?: boolean
  appId?: string
  wholeScreen?: boolean
  notGrantable?: NotGrantable
}
export type GrantChange =
  | "granted"
  | "revoked"
  | "target-changed"
  | "expired"
  | "disabled"
  | "stopped"
export interface ComputerGrantPayload {
  targetId: string
  change: GrantChange
  level: GrantLevel
}
export type ComputerAction =
  | "capture"
  | "snapshot"
  | "verify"
  | "click"
  | "drag"
  | "scroll"
  | "type"
  | "key"
  | "hold-key"
  | "set-value"
  | "restore"
  | "menu"
  | "set-frame"
  | "launch"
  | "clipboard-read"
  | "clipboard-write"
export type ActivityOutcome = "done" | "refused" | "failed"
export interface ComputerActivityPayload {
  actor?: { connectionId: string; agent: string }
  targetId: string
  action: ComputerAction
  outcome: ActivityOutcome
  at: number
  app?: string
}
export interface DefaultBlock {
  key: string
  name: string
  names: string[]
}
export interface ComputerToolsSettings {
  enabled: boolean
  grantTtlMinutes: number
  blocklist: string[]
  blocklistRemoved: string[]
  blocklistDefaults: DefaultBlock[]
  stopShortcut: string
  showIndicator: boolean
  allowForeground: boolean
  launchEnabled?: boolean
  clipboardEnabled?: boolean
  screenEnabled?: boolean
  defaultDelivery: ComputerDelivery
}
export type ComputerDelivery = "background" | "foreground"
export interface StopKeyStatus {
  active?: string
  failed?: string
  detail?: string
}
export interface ComputerMarkerPayload {
  id: number
  action: ComputerAction
}
export interface ShareManyResult {
  shared: SharedWindow[]
  skipped: number
}
export type DriverTask =
  | {
      kind: "installing"
      downloadedMb?: number
      totalMb?: number
    }
  | {
      kind: "uninstalling"
    }
export interface DriverInfo {
  version: string
  supported: boolean
  installed: string[]
  path?: string
  task?: DriverTask
  error?: string
}
export interface ComputerStatePayload {
  shared: SharedWindow[]
  apps?: SharedApp[]
  screen?: SharedScreen
}
export const COMPUTER_STATE_EVENT = "computer://state"
export const COMPUTER_GRANT_EVENT = "computer://agent-grant"
export const COMPUTER_ACTIVITY_EVENT = "computer://agent-activity"
export const COMPUTER_BACKEND_STATUS_EVENT = "computer://backend-status"
export const COMPUTER_STOP_KEY_EVENT = "computer://stop-key"
export const COMPUTER_MARKER_EVENT = "computer://marker"
export const COMPUTER_DRIVER_EVENT = "computer://driver"
export const COMPUTER_TOOLS_SETTINGS_CHANGED_EVENT =
  "computer-tools-settings://changed"
