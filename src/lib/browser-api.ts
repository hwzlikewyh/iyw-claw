import type { Channel } from "@tauri-apps/api/core"
import { getShellTransport, isDesktop } from "@/lib/transport"
import type {
  BrowserFrameSubscriptionSnapshot,
  BrowserGenerations,
  BrowserHostRegistration,
  BrowserInputEvent,
  BrowserStateSnapshot,
  BrowserViewClaimSnapshot,
} from "@/lib/browser-types"

const shell = () => getShellTransport()

export const browserApi = {
  state: () => shell().call<BrowserStateSnapshot>("browser_get_state"),
  refreshCapability: () =>
    shell().call<BrowserStateSnapshot>("browser_refresh_capability"),
  start: () => shell().call<BrowserStateSnapshot>("browser_start_runtime"),
  stop: () => shell().call<BrowserStateSnapshot>("browser_stop_runtime"),
  createTab: (url: string, hostId?: string) =>
    shell().call<BrowserStateSnapshot>("browser_create_tab", {
      url,
      hostId: hostId ?? null,
    }),
  ensureInitialTab: (url: string, hostId?: string) =>
    shell().call<BrowserStateSnapshot>("browser_ensure_initial_tab", {
      url,
      hostId: hostId ?? null,
    }),
  closeTab: (tabId: string) =>
    shell().call<BrowserStateSnapshot>("browser_close_tab", { tabId }),
  navigate: (tabId: string, url: string) =>
    shell().call<BrowserStateSnapshot>("browser_navigate_tab", { tabId, url }),
  back: (tabId: string) =>
    shell().call<BrowserStateSnapshot>("browser_back", { tabId }),
  forward: (tabId: string) =>
    shell().call<BrowserStateSnapshot>("browser_forward", { tabId }),
  reload: (tabId: string) =>
    shell().call<BrowserStateSnapshot>("browser_reload_tab", { tabId }),
  resize: (
    tabId: string,
    generations: BrowserGenerations,
    width: number,
    height: number,
    scale: number
  ) =>
    shell().call<BrowserStateSnapshot>("browser_resize_viewport", {
      tabId,
      generations,
      width,
      height,
      scale,
    }),
  registerHost: (windowLabel: string, kind: "docked" | "detached") =>
    shell().call<BrowserHostRegistration>("browser_register_host", {
      windowLabel,
      kind,
    }),
  heartbeatHost: (hostId: string, generation: number, visible: boolean) =>
    shell().call<BrowserStateSnapshot>("browser_heartbeat_host", {
      hostId,
      generation,
      visible,
    }),
  unregisterHost: (hostId: string) =>
    shell().call<BrowserStateSnapshot>("browser_unregister_host", { hostId }),
  activateTab: (hostId: string, hostGeneration: number, tabId: string) =>
    shell().call<BrowserStateSnapshot>("browser_activate_tab", {
      hostId,
      hostGeneration,
      tabId,
    }),
  createWindow: () => shell().call<string>("browser_create_window"),
  closeWindow: (windowLabel: string) =>
    shell().call<void>("browser_close_window", { windowLabel }),
  closeWindowPreservingTabs: (windowLabel: string) =>
    shell().call<void>("browser_close_window_preserving_tabs", {
      windowLabel,
    }),
  focusWindow: (windowLabel: string) =>
    shell().call<void>("browser_focus_window", { windowLabel }),
  completeWindowOpen: (requestId: string) =>
    shell().call<BrowserStateSnapshot>("browser_complete_window_open", {
      requestId,
    }),
  completeWindowClose: (requestId: string) =>
    shell().call<BrowserStateSnapshot>("browser_complete_window_close", {
      requestId,
    }),
  beginClaim: (
    tabId: string,
    sourceHostId: string | undefined,
    targetHostId: string,
    targetIndex: number
  ) =>
    shell().call<BrowserViewClaimSnapshot>("browser_begin_view_claim", {
      tabId,
      sourceHostId: sourceHostId ?? null,
      targetHostId,
      targetIndex,
    }),
  abortClaim: (claimId: string, generations: BrowserGenerations) =>
    shell().call<BrowserStateSnapshot>("browser_abort_view_claim", {
      claimId,
      generations,
    }),
  setUserHeld: (tabId: string, held: boolean) =>
    shell().call<BrowserStateSnapshot>("browser_set_user_held", {
      tabId,
      held,
    }),
  sendInput: (
    subscriptionId: string,
    generations: BrowserGenerations,
    events: BrowserInputEvent[]
  ) =>
    shell().call<void>("browser_send_input", {
      subscriptionId,
      generations,
      events,
    }),
  chooseFiles: (
    chooserId: string,
    generations: BrowserGenerations,
    paths: string[]
  ) =>
    shell().call<BrowserStateSnapshot>("browser_choose_files", {
      chooserId,
      generations,
      paths,
    }),
  answerDialog: (
    dialogId: string,
    generations: BrowserGenerations,
    accept: boolean,
    promptText?: string
  ) =>
    shell().call<BrowserStateSnapshot>("browser_answer_dialog", {
      dialogId,
      generations,
      accept,
      promptText: promptText ?? null,
    }),
  cancelDownload: (downloadId: string) =>
    shell().call<BrowserStateSnapshot>("browser_cancel_download", {
      downloadId,
    }),
  openDownload: (downloadId: string) =>
    shell().call<void>("browser_open_download", { downloadId }),
  revealDownload: (downloadId: string) =>
    shell().call<void>("browser_reveal_download", { downloadId }),
}

export interface BrowserFrameChannel {
  channel: Channel<ArrayBuffer | Uint8Array | number[]>
  subscription: BrowserFrameSubscriptionSnapshot
}

export async function subscribeBrowserFrames(
  tabId: string,
  generations: BrowserGenerations,
  claimId: string | undefined,
  onFrame: (value: ArrayBuffer | Uint8Array | number[]) => void
): Promise<BrowserFrameChannel> {
  if (!isDesktop()) throw new Error("Browser frames require Tauri desktop")
  const { Channel } = await import("@tauri-apps/api/core")
  const channel = new Channel<ArrayBuffer | Uint8Array | number[]>()
  channel.onmessage = onFrame
  const command = claimId
    ? "browser_subscribe_claim_frames"
    : "browser_subscribe_frames"
  const args = claimId
    ? { claimId, generations, onFrame: channel }
    : { tabId, generations, onFrame: channel }
  const subscription = await shell().call<BrowserFrameSubscriptionSnapshot>(
    command,
    args
  )
  return { channel, subscription }
}

export async function acknowledgeBrowserFrame(
  subscriptionId: string,
  generations: BrowserGenerations,
  seq: number,
  claimId?: string
): Promise<BrowserViewClaimSnapshot | void> {
  if (claimId) {
    return shell().call<BrowserViewClaimSnapshot>("browser_ack_claim_frame", {
      claimId,
      subscriptionId,
      generations,
      seq,
    })
  }
  await shell().call<void>("browser_ack_frame", {
    subscriptionId,
    generations,
    seq,
  })
}

export function commitBrowserClaim(
  claimId: string,
  subscriptionId: string,
  generations: BrowserGenerations
): Promise<BrowserStateSnapshot> {
  return shell().call("browser_commit_view_claim", {
    claimId,
    subscriptionId,
    generations,
  })
}

export function unsubscribeBrowserFrames(
  subscriptionId: string,
  generations: BrowserGenerations
): Promise<void> {
  return shell().call("browser_unsubscribe_frames", {
    subscriptionId,
    generations,
  })
}

export function getBrowserFrameSubscription(
  subscriptionId: string,
  generations: BrowserGenerations
): Promise<BrowserFrameSubscriptionSnapshot> {
  return shell().call("browser_get_frame_subscription", {
    subscriptionId,
    generations,
  })
}
