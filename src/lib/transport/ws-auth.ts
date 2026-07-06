export const IYW_CLAW_WS_PROTOCOL = "iyw-claw-events"
const IYW_CLAW_WS_TOKEN_PROTOCOL_PREFIX = "iyw-claw-token."

function base64UrlEncode(value: string): string {
  const bytes = new TextEncoder().encode(value)
  let binary = ""
  for (const byte of bytes) {
    binary += String.fromCharCode(byte)
  }
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "")
}

export function buildIywClawWebSocketProtocols(token: string): string[] {
  const trimmed = token.trim()
  if (!trimmed) return [IYW_CLAW_WS_PROTOCOL]
  return [
    IYW_CLAW_WS_PROTOCOL,
    `${IYW_CLAW_WS_TOKEN_PROTOCOL_PREFIX}${base64UrlEncode(trimmed)}`,
  ]
}
