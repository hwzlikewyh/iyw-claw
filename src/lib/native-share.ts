import { getShellTransport } from "@/lib/transport"

export const OPEN_NATIVE_SHARE = "iyw-claw:open-native-share"
export const NATIVE_SHARE_DRAFT = "iyw-claw:native-share-draft"
export const NATIVE_SHARE_COUNT = "iyw-claw:native-share-count"
let pendingNativeShares = 0

export const nativeSharePendingCount = () => pendingNativeShares

export function updateNativeShareCount(count: number): void {
  pendingNativeShares = count
  window.dispatchEvent(new CustomEvent(NATIVE_SHARE_COUNT, { detail: count }))
}

export interface ShareItem {
  name: string
  relativePath: string
  size: number
}

export interface ShareBatch {
  schemaVersion: number
  id: string
  createdAt: string
  items: ShareItem[]
  imported: boolean
  error: string | null
}

export interface SharePreview {
  id: string
  preview: string
  messageCount: number | null
  attachmentCount: number
  warning: string | null
  files: ShareItem[]
  paths: string[]
}

export interface ShareStatus {
  platform: "windows" | "macos" | "other"
  installed: boolean
  canRegister: boolean
}

export interface ShareDraftRequest {
  tabId: string
  batchId: string
  prompt: string
  paths: string[]
  finish: (error?: Error) => void
}

export const listNativeShares = () =>
  getShellTransport().call<ShareBatch[]>("native_share_list")

export const prepareNativeShare = (id: string) =>
  getShellTransport().call<SharePreview>("native_share_prepare", { id })

export const completeNativeShare = (id: string) =>
  getShellTransport().call<void>("native_share_complete", { id })

export const importNativeShare = (paths: string[]) =>
  getShellTransport().call<ShareBatch>("native_share_import", { paths })

export const nativeShareStatus = () =>
  getShellTransport().call<ShareStatus>("native_share_status")

export const registerNativeShare = () =>
  getShellTransport().call<void>("native_share_register")

export function openNativeShareInbox(): void {
  window.dispatchEvent(new Event(OPEN_NATIVE_SHARE))
}

export function addNativeShareToDraft(
  request: Omit<ShareDraftRequest, "finish">,
  unavailable: string
): Promise<void> {
  return new Promise((resolve, reject) => {
    const detail: ShareDraftRequest = {
      ...request,
      finish: (error) => (error ? reject(error) : resolve()),
    }
    const claimed = !window.dispatchEvent(
      new CustomEvent<ShareDraftRequest>(NATIVE_SHARE_DRAFT, {
        detail,
        cancelable: true,
      })
    )
    if (!claimed) reject(new Error(unavailable))
  })
}

export function shareError(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === "string") return error
  if (error && typeof error === "object" && "message" in error) {
    return String(error.message)
  }
  return String(error)
}
