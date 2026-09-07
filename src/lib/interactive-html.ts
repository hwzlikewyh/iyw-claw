import { getTransport } from "@/lib/transport"

export const MAX_HTML_BYTES = 256 * 1024
export const MAX_HTML_RESULT_BYTES = 64 * 1024
export const HTML_LOAD_TIMEOUT_MS = 15000
export const HTML_SUBMIT_TIMEOUT_MS = 30000
export const HTML_MIN_HEIGHT = 160
export const HTML_MAX_HEIGHT = 1200
export const HTML_DEFAULT_HEIGHT = 360
export const HTML_NONCE_WORDS = 4
export const HTML_MESSAGE_ID_CHARS = 32
export type HtmlErrorKey =
  | "loadError"
  | "pageError"
  | "submitError"
  | "tooLarge"

export type HtmlResponse = {
  interactionId: string
  action: "submit" | "text" | "close"
  data?: unknown
}

export function respondInteractiveHtml(
  connectionId: string,
  response: HtmlResponse
): Promise<void> {
  return getTransport().call("acp_respond_html", { connectionId, response })
}

export function serializedBytes(value: unknown): number {
  const serialized = JSON.stringify(value)
  if (serialized === undefined) throw new Error("A JSON value is required")
  return new TextEncoder().encode(serialized).byteLength
}
