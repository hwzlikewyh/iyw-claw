// 账单异步落库时有限补读；每次收到新用量信号后重新计算重试预算。
export const USAGE_RETRY_DELAYS_MS = [
  1_000, 3_000, 5_000, 10_000, 15_000,
] as const

const METADATA_SETTLE_MS = 250
const NATIVE_PERSIST_SETTLE_MS = 1_500
const NATIVE_METADATA_RETRY_MS = 3_000

export function metadataRefreshDelay(attempt: number, isCodex: boolean) {
  if (isCodex)
    return attempt === 0
      ? METADATA_SETTLE_MS
      : USAGE_RETRY_DELAYS_MS[attempt - 1]
  return attempt === 0 ? NATIVE_PERSIST_SETTLE_MS : NATIVE_METADATA_RETRY_MS
}
