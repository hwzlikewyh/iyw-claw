/**
 * Shared attachment value types for the message input.
 *
 * Extracted from `message-input.tsx` so the host component and the composer's
 * send/restore serializers ({@link "./composer/to-prompt-blocks"} /
 * {@link "./composer/from-prompt-blocks"}) all agree on one definition rather
 * than re-declaring structurally-compatible copies.
 *
 * An attachment is content the user adds *out of band* of the prose — pasted /
 * dragged / uploaded / picked images and files. Inline references typed via the
 * `@` panel are NOT attachments; they live in the editor document as reference
 * badges. Both fold into the outgoing `PromptInputBlock[]` at send time.
 */

/** A file/resource attachment (a `file://` link, an uploaded blob, or an
 *  embedded text/binary resource). */
export interface ResourceInputAttachment {
  id: string
  type: "resource"
  /** `link` → sent as a ResourceLink (uri only); `embedded` → sent as a Resource
   *  carrying inline `text`/`blob`. */
  kind: "link" | "embedded"
  uri: string
  name: string
  mimeType: string | null
  text?: string | null
  blob?: string | null
}

/** An image attachment. New sends use an HTTPS `uri` and empty `data`; non-empty
 *  base64 remains supported only for restored legacy drafts. */
export interface ImageInputAttachment {
  id: string
  type: "image"
  data: string
  uri: string | null
  name: string
  mimeType: string
  /** MIME type of the original file when the derived image was re-encoded. */
  sourceMimeType?: string
  /** Runtime-only retry state. It is intentionally omitted from prompt blocks. */
  staging?: ImageAttachmentStaging
}

export type ImageAttachmentStaging = {
  status: "failed" | "uploading"
  source:
    | { kind: "browser-file"; file: File }
    | { kind: "local-path"; path: string }
}

export type InputAttachment = ResourceInputAttachment | ImageInputAttachment
