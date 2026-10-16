import type { ReferenceAttrs } from "@/components/chat/composer/types"

export const ATTACH_FILE_TO_SESSION_EVENT = "iyw-claw:attach-file-to-session"

export interface AttachFileToSessionDetail {
  tabId: string
  path: string
  /**
   * Optional 1-based, inclusive line span to attach as a ranged file badge
   * (`foo.ts:10-25`). Omitted by whole-file callers (file tree, git changes);
   * supplied by the editor's "add selection to chat". When present the consumer
   * encodes it into the badge uri (`file://…#L10-25`) and label.
   */
  range?: { start: number; end: number }
}

export function emitAttachFileToSession(
  detail: AttachFileToSessionDetail
): void {
  if (typeof window === "undefined") return
  window.dispatchEvent(
    new CustomEvent<AttachFileToSessionDetail>(ATTACH_FILE_TO_SESSION_EVENT, {
      detail,
    })
  )
}

export const APPEND_TEXT_TO_SESSION_EVENT = "iyw-claw:append-text-to-session"

export interface AppendTextToSessionDetail {
  tabId: string
  text: string
}

export function emitAppendTextToSession(
  detail: AppendTextToSessionDetail
): boolean {
  if (typeof window === "undefined") return false
  return !window.dispatchEvent(
    new CustomEvent<AppendTextToSessionDetail>(APPEND_TEXT_TO_SESSION_EVENT, {
      detail,
      cancelable: true,
    })
  )
}

export const INSERT_REFERENCE_TO_SESSION_EVENT =
  "iyw-claw:insert-reference-to-session"

export interface InsertReferenceToSessionDetail {
  tabId: string
  reference: ReferenceAttrs
}

export function emitInsertReferenceToSession(
  detail: InsertReferenceToSessionDetail
): boolean {
  if (typeof window === "undefined") return false
  return !window.dispatchEvent(
    new CustomEvent<InsertReferenceToSessionDetail>(
      INSERT_REFERENCE_TO_SESSION_EVENT,
      { detail, cancelable: true }
    )
  )
}

export const ATTACH_IMAGE_TO_SESSION_EVENT = "iyw-claw:attach-image-to-session"

export interface AttachImageToSessionDetail {
  tabId: string
  data: string
  mimeType: string
  name: string
}

export function emitAttachImageToSession(
  detail: AttachImageToSessionDetail
): boolean {
  if (typeof window === "undefined") return false
  return !window.dispatchEvent(
    new CustomEvent<AttachImageToSessionDetail>(ATTACH_IMAGE_TO_SESSION_EVENT, {
      detail,
      cancelable: true,
    })
  )
}
