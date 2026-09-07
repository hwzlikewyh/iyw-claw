import type { QuestionSpec } from "@/lib/types"

export function inputDefaults(question: QuestionSpec) {
  const defaults = question.secret ? [] : (question.input?.default_values ?? [])
  const labels = new Set(question.options.map((option) => option.label))
  return {
    chosen: defaults.filter((value) => labels.has(value)),
    otherText: defaults.filter((value) => !labels.has(value)).join(""),
    otherActive:
      question.options.length === 0 ||
      defaults.some((value) => !labels.has(value)),
  }
}

export function allowsEmptyInput(question: QuestionSpec): boolean {
  const schema = question.input?.schema
  return (
    (schema?.type === "string" &&
      question.options.length === 0 &&
      !(schema.minLength ?? 0)) ||
    (schema?.type === "array" && !(schema.minItems ?? 0))
  )
}

export function inputHint(question: QuestionSpec): string | undefined {
  const schema = question.input?.schema
  if (!schema) return undefined
  const parts: string[] = []
  if (question.optional) parts.push("可留空")
  if (schema.type === "integer") parts.push("请输入整数")
  if (schema.type === "number") parts.push("请输入数字")
  if (schema.format) {
    const formats = {
      email: "电子邮箱",
      uri: "完整网址或 URI",
      date: "日期 YYYY-MM-DD",
      "date-time": "包含时区的日期和时间",
    }
    parts.push(formats[schema.format])
  }
  for (const [value, label] of [
    [schema.minLength, "最少字符数"],
    [schema.maxLength, "最多字符数"],
    [schema.minimum, "最小值"],
    [schema.maximum, "最大值"],
    [schema.minItems, "最少选择数"],
    [schema.maxItems, "最多选择数"],
  ] as const) {
    if (value !== undefined) parts.push(`${label}：${value}`)
  }
  if (schema.pattern) parts.push(`格式要求：${schema.pattern}`)
  return parts.length ? parts.join("；") : undefined
}

export function preservesEmptyText(question: QuestionSpec): boolean {
  return (
    question.input?.schema.type === "string" &&
    (!question.optional || question.input.default_values.includes(""))
  )
}

export function answerError(error: unknown, fallback: string): string {
  if (typeof error === "string") return error
  if (error instanceof Error) return error.message
  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  )
    return error.message
  return fallback
}
