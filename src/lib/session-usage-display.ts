import type { GatewayModel } from "@/lib/gateway-model-parser"
import type {
  AgentType,
  SessionStats,
  SessionUsageUpdateInfo,
} from "@/lib/types"

export type TokenRowKey =
  | "input"
  | "output"
  | "cacheRead"
  | "cacheWrite"
  | "total"
export type UsageSource = "reported" | "history" | "awaiting" | "unavailable"
export type CompactionMode =
  | "nativeTokens"
  | "nativeWindow"
  | "nativeRatio"
  | "agentManaged"

export interface SessionUsageData {
  modelId: string | null
  contextUsed: number | null
  contextMax: number | null
  contextPercent: number | null
  contextSource: UsageSource
  rows: { key: TokenRowKey; value: number | null }[]
  total: number | null
  points: number | null
  model: GatewayModel | null
  threshold: number | null
  hostThreshold: number | null
  thresholdSource: "catalog" | "host" | "unavailable"
  compactionMode: CompactionMode
  configStale: boolean
  thresholdReached: boolean
}

export interface SessionUsageInput {
  agentType: AgentType | null
  modelId: string | null
  stats: SessionStats | null
  liveUsage?: SessionUsageUpdateInfo | null
  hostThreshold?: number | null
  configStale?: boolean
  model: GatewayModel | null
}

export function tokenValue(value: number | null | undefined): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : null
}

function positiveTokenValue(value: number | null | undefined): number | null {
  const tokens = tokenValue(value)
  return tokens !== null && tokens > 0 ? tokens : null
}

function resolveContext(input: SessionUsageInput) {
  const liveMax = positiveTokenValue(input.liveUsage?.size)
  if (input.liveUsage) {
    const reported = tokenValue(input.liveUsage.used)
    // Claude ACP 在读取压缩后用量失败时用 0 占位，不能恢复旧值或宣称已清空。
    const used =
      input.agentType === "claude_code" && reported === 0 ? null : reported
    return {
      used,
      max: liveMax,
      source: used === null ? "awaiting" : "reported",
    } as const
  }
  const used =
    input.agentType === "grok"
      ? null
      : tokenValue(input.stats?.context_window_used_tokens)
  const max = positiveTokenValue(input.stats?.context_window_max_tokens)
  return {
    used,
    max,
    source: used !== null || max !== null ? "history" : "unavailable",
  } as const
}

function usageRows(stats: SessionStats | null, agentType: AgentType | null) {
  // Grok 历史只提供 totalTokens，解析器中的输入占位不能当作真实分类。
  const usage = agentType === "grok" ? null : stats?.total_usage
  const rows: SessionUsageData["rows"] = [
    { key: "input", value: tokenValue(usage?.input_tokens) },
    { key: "output", value: tokenValue(usage?.output_tokens) },
    { key: "cacheRead", value: tokenValue(usage?.cache_read_input_tokens) },
    {
      key: "cacheWrite",
      value: tokenValue(usage?.cache_creation_input_tokens),
    },
  ]
  // 后端已将缓存和普通输入归一为互斥分类，前端不再次加到输入里。
  const sum = rows.every((row) => row.value !== null)
    ? tokenValue(rows.reduce((sum, row) => sum + (row.value ?? 0), 0))
    : null
  const total = tokenValue(stats?.total_tokens) ?? sum
  return { rows: [...rows, { key: "total" as const, value: total }], total }
}

function compactionMode(agent: AgentType | null): CompactionMode {
  if (agent === "codex") return "nativeTokens"
  if (agent === "claude_code") return "nativeWindow"
  if (agent === "kimi_code" || agent === "gemini") return "nativeRatio"
  return "agentManaged"
}

export function resolveSessionUsage(
  input: SessionUsageInput
): SessionUsageData {
  const context = resolveContext(input)
  const configured = positiveTokenValue(input.model?.compactionAtTokens)
  const host = positiveTokenValue(input.hostThreshold)
  const threshold = configured ?? host
  return {
    modelId: input.modelId,
    contextUsed: context.used,
    contextMax: context.max,
    contextPercent:
      context.used !== null && context.max !== null
        ? (context.used / context.max) * 100
        : null,
    contextSource: context.source,
    ...usageRows(input.stats, input.agentType),
    points:
      input.agentType === "grok"
        ? null
        : (input.stats?.total_usage?.estimated_points ?? null),
    model: input.model,
    threshold,
    hostThreshold: host,
    thresholdSource:
      configured !== null ? "catalog" : host !== null ? "host" : "unavailable",
    compactionMode: compactionMode(input.agentType),
    configStale: input.configStale === true,
    thresholdReached:
      threshold !== null && context.used !== null && context.used >= threshold,
  }
}

export function findUsageModel(models: GatewayModel[], id: string | null) {
  if (!id) return null
  const normalized = id.replace(/^iyw-claw\//, "").toLowerCase()
  return models.find((model) => model.id.toLowerCase() === normalized) ?? null
}
