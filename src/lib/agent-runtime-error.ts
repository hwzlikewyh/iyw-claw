export interface AgentRuntimeErrorMessages {
  insufficientBalance: string
  authenticationFailed: string
  permissionDenied: string
  rateLimited: string
  quotaExceeded: string
  modelUnavailable: string
  requestTimeout: string
  networkError: string
  serviceUnavailable: string
  requestFailed: string
}

type AgentRuntimeErrorKind =
  | "insufficientBalance"
  | "authenticationFailed"
  | "permissionDenied"
  | "rateLimited"
  | "quotaExceeded"
  | "modelUnavailable"
  | "requestTimeout"
  | "networkError"
  | "serviceUnavailable"
  | "requestFailed"

const CATEGORY_PATTERNS: ReadonlyArray<
  readonly [AgentRuntimeErrorKind, readonly RegExp[]]
> = [
  [
    "insufficientBalance",
    [
      /insufficient[ _-]+balance/i,
      /payment required/i,
      /credit balance (?:is )?too low/i,
      /余额不足/,
    ],
  ],
  [
    "modelUnavailable",
    [
      /has not activated the model/i,
      /model service (?:is )?not activated/i,
      /model[_ -]not[_ -]found/i,
      /model .{0,80}(?:does not exist|is not available|is unavailable)/i,
      /(?:do not|don't) have access to (?:the )?model/i,
      /模型.{0,20}(?:未开通|不可用|不存在|无权访问)/,
    ],
  ],
  [
    "authenticationFailed",
    [
      /\bunauthorized\b/i,
      /invalid[ _-]+api[ _-]+key/i,
      /authentication (?:failed|required)/i,
      /(?:invalid|expired)[ _-]+(?:access[ _-]+)?token/i,
      /token .{0,30}(?:is )?(?:invalid|expired)/i,
      /(?:认证失败|密钥无效|令牌已过期)/,
    ],
  ],
  [
    "permissionDenied",
    [
      /\bforbidden\b/i,
      /permission denied/i,
      /access denied/i,
      /(?:没有权限|无权访问)/,
    ],
  ],
  [
    "quotaExceeded",
    [
      /insufficient[ _-]+quota/i,
      /quota (?:has been )?(?:exceeded|exhausted)/i,
      /usage limit (?:has been )?(?:reached|exceeded)/i,
      /(?:额度已用完|配额不足|超出配额)/,
    ],
  ],
  [
    "rateLimited",
    [
      /rate[ _-]+limit/i,
      /too many requests/i,
      /request limit (?:has been )?exceeded/i,
      /\bthrottl(?:e|ed|ing)\b/i,
      /请求过于频繁/,
    ],
  ],
  [
    "requestTimeout",
    [/timed? out/i, /\btimeout\b/i, /deadline exceeded/i, /请求超时/],
  ],
  [
    "networkError",
    [
      /network error/i,
      /connection (?:reset|refused|closed)/i,
      /(?:dns|name resolution) (?:error|failed|failure)/i,
      /tls handshake (?:error|failed|failure)/i,
      /failed to fetch/i,
      /网络(?:连接)?(?:错误|失败)/,
    ],
  ],
  [
    "serviceUnavailable",
    [
      /service unavailable/i,
      /bad gateway/i,
      /internal server error/i,
      /gateway timeout/i,
      /server (?:is )?overloaded/i,
      /服务(?:暂时)?不可用/,
    ],
  ],
]

function looksLikeRuntimeError(message: string): boolean {
  const startsLikeError =
    /^(?:unexpected status|https?(?: status| error)?\s*[:=]?\s*\d{3}\b|api(?:status)?error\s*:|error(?: code)?\s*:|\d{3}\s+[a-z]|request .{0,40}(?:failed|timed out)|network error|insufficient[ _-]+(?:balance|quota)|payment required|your account .{0,120}has not activated the model)/i
  const hasOpaqueRequestId =
    /request[ _-]?id\s*:\s*[a-z0-9][a-z0-9-]{7,}/i.test(message)
  const hasLabeledUrl = /\burl\s*:\s*https?:\/\/\S+/i.test(message)
  const hasTransportMarker =
    /(?:error|fail|status|unauthorized|forbidden|quota|balance|timeout)/i.test(
      message
    )
  const hasDiagnosticFields =
    hasOpaqueRequestId || (hasLabeledUrl && hasTransportMarker)

  return (
    startsLikeError.test(message) ||
    hasDiagnosticFields ||
    isJsonErrorPayload(message)
  )
}

function isJsonErrorPayload(message: string): boolean {
  if (!message.startsWith("{")) return false
  try {
    const parsed: unknown = JSON.parse(message)
    return (
      !!parsed &&
      typeof parsed === "object" &&
      !Array.isArray(parsed) &&
      "error" in parsed
    )
  } catch {
    return false
  }
}

function extractHttpStatus(message: string): number | null {
  const match = message.match(
    /\b(?:unexpected status|https?(?: status| error)?|status code|api error|error code)\s*[:=]?\s*(\d{3})\b/i
  )
  if (match) return Number(match[1])

  const leading = message.match(/^\s*(\d{3})\s+[a-z]/i)
  return leading ? Number(leading[1]) : null
}

export function classifyAgentRuntimeError(
  message: string
): AgentRuntimeErrorKind | null {
  const normalized = message.trim()
  if (!normalized || !looksLikeRuntimeError(normalized)) return null

  for (const [kind, patterns] of CATEGORY_PATTERNS) {
    if (patterns.some((pattern) => pattern.test(normalized))) return kind
  }

  const status = extractHttpStatus(normalized)
  if (status === 401) return "authenticationFailed"
  if (status === 402) return "insufficientBalance"
  if (status === 403) return "permissionDenied"
  if (status === 408 || status === 504) return "requestTimeout"
  if (status === 429) return "rateLimited"
  if (status !== null && status >= 500) return "serviceUnavailable"
  return "requestFailed"
}

export function formatAgentRuntimeError(
  message: string,
  messages: AgentRuntimeErrorMessages
): string | null {
  const kind = classifyAgentRuntimeError(message)
  return kind ? messages[kind] : null
}
