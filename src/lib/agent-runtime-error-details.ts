/** 具体请求错误优先于通用 HTTP 状态与网络分类。 */
export const REQUEST_ERROR_PATTERNS = [
  [
    "workspaceSwitching",
    [/workspace_switch(?:ing|_pending)|空间正在切换|空间切换尚未确认/i],
  ],
  [
    "contextTooLong",
    [
      /context_(?:length|window)_exceeded|input_too_long/i,
      /maximum context length|(?:context|prompt|input).{0,25}(?:too (?:long|large)|exceeds? .{0,20}(?:limit|length))/i,
      /上下文.{0,15}(?:超出|超过|过长)|输入.{0,15}(?:过长|超过.{0,10}限制)/,
    ],
  ],
  [
    "requestTooLarge",
    [
      /(?:request|payload|entity|file|image).{0,20}too large|request_too_large/i,
      /(?:消息|请求|附件|图片|文件).{0,10}(?:过大|超过.{0,10}大小限制)/,
    ],
  ],
  [
    "contentRejected",
    [
      /content_(?:filter|policy_violation)|responsible_?ai_?(?:policy_?)?violation/i,
      /(?:content|request).{0,35}(?:blocked|rejected).{0,35}(?:safety|policy)|内容.{0,15}(?:审核未通过|被拦截|不符合.{0,10}要求)/i,
    ],
  ],
  [
    "modelOverloaded",
    [
      /overloaded_error|(?:model|server|service).{0,20}(?:overloaded|at capacity)|capacity exhausted/i,
      /(?:模型|服务).{0,10}(?:繁忙|过载)|服务器负载过高/,
    ],
  ],
  [
    "sessionUnavailable",
    [
      /session_(?:not_found|expired)|thread_not_found/i,
      /(?:session|thread|conversation).{0,40}(?:not found|does not exist|expired|unavailable)|会话.{0,15}(?:不存在|已过期|无法恢复)/i,
    ],
  ],
  [
    "toolProtocolError",
    [
      /tool_protocol_violation|tool_use ids? were found without tool_result/i,
      /tool_call_id.{0,40}(?:not found|missing|invalid)|tool_calls.{0,80}(?:must|followed)/i,
      /工具调用.{0,15}(?:格式错误|不匹配|无效)|缺少工具.{0,5}(?:结果|响应)/,
    ],
  ],
  [
    "outputLimitReached",
    [
      /reason[:=]\s*["']?max_output_tokens\b|(?:output|response).{0,30}(?:token|length) limit (?:reached|exceeded)/i,
      /(?:回复|输出).{0,15}(?:达到|超过).{0,15}(?:上限|限制)/,
    ],
  ],
  [
    "modelStreamInterrupted",
    [
      /stream disconnected before completion|stream (?:closed|ended) (?:unexpectedly|prematurely)|incomplete chunked read/i,
      /(?:流式响应|回复|响应).{0,10}(?:中途断开|意外中断)/,
    ],
  ],
  [
    "invalidRequest",
    [
      /invalid_request_error|invalid_(?:argument|parameter)|unsupported_parameter|unprocessable entity/i,
      /(?:invalid|unsupported|missing required) (?:parameter|argument|request)|参数.{0,10}(?:无效|不支持|缺失)/i,
    ],
  ],
] as const

export type DetailedRuntimeErrorKind =
  (typeof REQUEST_ERROR_PATTERNS)[number][0]

/** 只把以具体错误特征开头的裸消息视为诊断，避免改写普通回复正文。 */
export function startsWithRequestError(message: string): boolean {
  return REQUEST_ERROR_PATTERNS.some(([, patterns]) =>
    patterns.some((pattern) => pattern.exec(message)?.index === 0)
  )
}
