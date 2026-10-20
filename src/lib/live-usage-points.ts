import type { LiveMessage } from "@/contexts/acp-connections-context"
import type { GatewayTokenPrice } from "@/lib/gateway-model-parser"
import type { TurnUsage } from "@/lib/types"

const LEGACY_POINTS_PER_CNY = 10
const TOKENS_PER_MILLION = 1_000_000
const LATIN_CHARS_PER_TOKEN = 4
const TOKEN_SEGMENTS =
  /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]|[\p{Extended_Pictographic}]|[\p{L}\p{N}]+|[^\s]/gu
const EAST_ASIAN_OR_EMOJI =
  /^[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Extended_Pictographic}]$/u

export function estimateLiveUsagePoints(input: {
  message: LiveMessage | null
  modelPrice: GatewayTokenPrice | null
  promptText?: string
  usage?: TurnUsage | null
}): number | null {
  const { message, modelPrice, usage } = input
  if (!modelPrice || !validPrice(modelPrice)) return null
  const output = outputText(message)
  const prompt = promptTextForMessage(message) || input.promptText || ""
  const outputTokens = estimateTokens(output.text + output.thinking)
  const promptTokens = estimateTokens(prompt)
  const reportedInput =
    (usage?.input_tokens ?? 0) +
    (usage?.cache_read_input_tokens ?? 0) +
    (usage?.cache_creation_input_tokens ?? 0)
  const pendingInput = Math.max(0, promptTokens - reportedInput)
  const pendingOutput = Math.max(0, outputTokens - (usage?.output_tokens ?? 0))
  const base = basePoints(usage, modelPrice)
  const pendingCost = pendingPoints(
    pendingInput,
    pendingOutput,
    output,
    modelPrice,
    usage?.output_tokens ?? 0
  )
  return base == null || pendingCost == null
    ? null
    : finitePoints(base + pendingCost)
}

function outputText(message: LiveMessage | null) {
  let text = ""
  let thinking = ""
  for (const block of message?.content ?? []) {
    if (block.type === "text" || block.type === "thinking") {
      if (block.type === "text") text += block.text
      else thinking += block.text
    }
  }
  return { text, thinking }
}

function promptTextForMessage(message: LiveMessage | null): string {
  return (message?.content ?? [])
    .flatMap((block) =>
      block.type === "user_input"
        ? block.blocks.flatMap((item) =>
            item.type === "text" ? [item.text] : []
          )
        : []
    )
    .join("")
}

function estimateTokens(text: string): number {
  const segments = text.match(TOKEN_SEGMENTS) ?? []
  return segments.reduce((count, segment) => {
    if (EAST_ASIAN_OR_EMOJI.test(segment)) return count + 1
    return count + Math.ceil(segment.length / LATIN_CHARS_PER_TOKEN)
  }, 0)
}

function basePoints(
  usage: TurnUsage | null | undefined,
  price: GatewayTokenPrice
): number | null {
  const confirmed = usage?.confirmed_points?.amount
  const value = confirmed ?? usage?.estimated_points
  if (value != null) return finitePoints(Number(value))
  return usage ? estimateReportedUsage(usage, price) : 0
}

function estimateReportedUsage(
  usage: TurnUsage,
  price: GatewayTokenPrice
): number | null {
  if (
    usage.output_tokens > 0 &&
    price.reasoning_output > 0 &&
    price.reasoning_output !== price.output
  )
    return null
  if (
    usage.cache_creation_input_tokens > 0 &&
    (price.cache_creation_5m > 0 || price.cache_creation_1h > 0)
  )
    return null
  const cost =
    usage.input_tokens * price.input +
    usage.output_tokens * price.output +
    usage.cache_read_input_tokens * price.cached_input +
    usage.cache_creation_input_tokens * price.cache_creation
  return toPoints(cost, price)
}

function pendingPoints(
  inputTokens: number,
  outputTokens: number,
  output: { text: string; thinking: string },
  price: GatewayTokenPrice,
  reportedOutput: number
) {
  if (
    price.reasoning_output > 0 &&
    price.reasoning_output !== price.output &&
    reportedOutput > 0
  )
    return toPoints(inputTokens * price.input, price)
  const outputCost =
    price.reasoning_output > 0 && price.reasoning_output !== price.output
      ? estimateTokens(output.text) * price.output +
        estimateTokens(output.thinking) * price.reasoning_output
      : outputTokens * price.output
  return toPoints(inputTokens * price.input + outputCost, price)
}

function toPoints(cost: number, price: GatewayTokenPrice): number | null {
  const rate =
    price.points_per_cny == null
      ? LEGACY_POINTS_PER_CNY
      : Number(price.points_per_cny)
  if (!Number.isFinite(rate) || rate < 0) return null
  return finitePoints((cost / TOKENS_PER_MILLION) * rate)
}

function validPrice(price: GatewayTokenPrice): boolean {
  return (
    price.currency === "CNY" &&
    price.unit === "per_1m_tokens" &&
    price.image_billing_mode !== "per_image" &&
    [
      price.input,
      price.output,
      price.cached_input,
      price.cache_creation,
      price.reasoning_output,
      price.cache_creation_5m,
      price.cache_creation_1h,
    ].every((value) => Number.isFinite(value) && value >= 0)
  )
}

function finitePoints(value: number): number | null {
  return Number.isFinite(value) && value >= 0 ? value : null
}
