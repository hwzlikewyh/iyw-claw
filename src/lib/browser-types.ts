export type BrowserRuntimeStatus =
  | "unsupported"
  | "missing"
  | "verifying"
  | "ready"
  | "starting"
  | "running"
  | "recovering"
  | "stopping"
  | "failed"

export type BrowserTabStatus =
  | "creating"
  | "live"
  | "navigating"
  | "crashed"
  | "gone"
  | "closing"
  | "closed"

export type BrowserViewStatus =
  | "unclaimed"
  | "attaching"
  | "docked"
  | "detaching"
  | "detached"

export interface BrowserGenerations {
  runtimeGeneration: number
  tabGeneration: number
  viewGeneration: number
  controlEpoch: number
}

export interface BrowserCapability {
  supported: boolean
  status: BrowserRuntimeStatus
  reason?: string
  platform: string
  architecture: string
  sidecarVersion: string
  sidecarVerified: boolean
  engine?: { kind: "chrome" | "edge"; version: string }
}

export interface BrowserTabSnapshot {
  browserTabId: string
  title: string
  url: string
  status: BrowserTabStatus
  viewStatus: BrowserViewStatus
  controlStatus:
    | "idle"
    | "agent_running"
    | "user_active"
    | "user_held"
    | "agent_waiting"
  documentEpoch: number
  generations: BrowserGenerations
  hostId?: string
}

export interface BrowserHostSnapshot {
  hostId: string
  windowLabel: string
  kind: "docked" | "detached"
  generation: number
  visible: boolean
  tabOrder: string[]
  activeTabId?: string
}

export interface BrowserViewClaimSnapshot {
  claimId: string
  browserTabId: string
  sourceHostId?: string
  targetHostId: string
  targetIndex: number
  targetStatus: BrowserViewStatus
  generations: BrowserGenerations
  firstFrameSeq?: number
  expiresInMs: number
}

export interface BrowserDialogSnapshot {
  dialogId: string
  browserTabId: string
  kind: "alert" | "confirm" | "prompt" | "before_unload"
  message: string
  defaultPrompt: string
  generations: BrowserGenerations
}

export interface BrowserFileChooserSnapshot {
  chooserId: string
  browserTabId: string
  mode: "select_single" | "select_multiple"
  generations: BrowserGenerations
}

export interface BrowserDownloadSnapshot {
  downloadId: string
  browserTabId?: string
  suggestedFilename: string
  status: "in_progress" | "completed" | "cancelled" | "failed"
  receivedBytes: number
  totalBytes?: number
  completedPath?: string
}

export interface BrowserStateSnapshot {
  stateRevision: number
  capability: BrowserCapability
  runtime: {
    status: BrowserRuntimeStatus
    generation: number
    operationId?: string
    failureCode?: string
  }
  tabs: BrowserTabSnapshot[]
  hosts: BrowserHostSnapshot[]
  dialogs: BrowserDialogSnapshot[]
  fileChoosers: BrowserFileChooserSnapshot[]
  downloads: BrowserDownloadSnapshot[]
  viewClaims: BrowserViewClaimSnapshot[]
}

export interface BrowserFrameSubscriptionSnapshot {
  subscriptionId: string
  browserTabId: string
  generations: BrowserGenerations
  status: "connecting" | "streaming" | "disconnected"
}

export interface BrowserHostRegistration {
  hostId: string
  generation: number
  state: BrowserStateSnapshot
}

export type BrowserInputEvent =
  | {
      kind: "mouse"
      eventType: "mouseMoved" | "mousePressed" | "mouseReleased" | "mouseWheel"
      x: number
      y: number
      button?: "none" | "left" | "right" | "middle"
      clickCount?: number
      deltaX?: number
      deltaY?: number
      modifiers?: number
    }
  | {
      kind: "keyboard"
      eventType: "keyDown" | "rawKeyDown" | "keyUp" | "char"
      key?: string
      code?: string
      text?: string
      windowsVirtualKeyCode?: number
      modifiers?: number
    }

export interface BrowserErrorEnvelope {
  code: string
  message: string
  retryable: boolean
  effectMayHaveOccurred: boolean
}
