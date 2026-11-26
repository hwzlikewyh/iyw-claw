import { SPACE_QUOTA_MESSAGE } from "@/lib/agent-runtime-error"

export type ImageErrorLabel =
  | "imageSpaceQuotaExceeded"
  | "imageServiceAccessFailed"
  | "imageSourceRequired"
  | "imageRequestInvalid"
  | "imageGenerationFailed"

function errorLabel(message: string): ImageErrorLabel {
  if (message.includes(SPACE_QUOTA_MESSAGE)) return "imageSpaceQuotaExceeded"
  if (message.includes("接口访问失败")) return "imageServiceAccessFailed"
  if (/(-32602|ImageToolRequest|invalid.*parameter)/i.test(message)) {
    return "imageRequestInvalid"
  }
  if (
    /\bimage edits? requires? (?:at least one|a) source image\b/i.test(message)
  ) {
    return "imageSourceRequired"
  }
  return "imageGenerationFailed"
}

function textErrors(value: string, failed: boolean): ImageErrorLabel[] {
  const text = value.trim()
  if (/^[[{"]/.test(text)) {
    try {
      return collectImageErrors(JSON.parse(text), failed)
    } catch {
      // 历史工具输出可能已按长度截断，仍可读取保留下来的 MCP 错误。
    }
  }
  return failed ||
    text === SPACE_QUOTA_MESSAGE ||
    text === "接口访问失败" ||
    /IYW image request failed|^(?:tool call error|Mcp error):/i.test(text)
    ? [errorLabel(text)]
    : []
}

function collectImageErrors(
  value: unknown,
  failed: boolean
): ImageErrorLabel[] {
  if (typeof value === "string") return textErrors(value, failed)
  if (Array.isArray(value))
    return value.flatMap((item) => collectImageErrors(item, failed))
  if (!value || typeof value !== "object") return []
  const record = value as Record<string, unknown>
  if (record.status === "succeeded") return []
  const details = record.error_details as { message?: unknown } | null
  const error = details?.message ?? record.error ?? record.failure
  if (error) return collectImageErrors(error, true)
  const hasError =
    failed ||
    record.status === "failed" ||
    record.isError === true ||
    record.is_error === true
  if (typeof record.message === "string")
    return textErrors(record.message, hasError)
  // 批量调用即使成功返回，也可能有部分图片生成失败。
  const metadata = record.metadata as { runs?: unknown } | null
  const child =
    record.items ??
    record.runs ??
    metadata?.runs ??
    record.structuredContent ??
    record.structured_content ??
    record.result ??
    record.content ??
    record.text
  return child != null ? collectImageErrors(child, hasError) : []
}

export function imageGenerationErrors(
  output: string | null | undefined,
  failed: boolean
): ImageErrorLabel[] {
  const labels = collectImageErrors(output, failed)
  // 仅保留需要用户处理的配额/访问问题；生成、参数和源图诊断由工具链处理。
  return labels.filter(
    (label) =>
      label === "imageSpaceQuotaExceeded" ||
      label === "imageServiceAccessFailed"
  )
}
