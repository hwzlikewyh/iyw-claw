import type { ConversationDetail, ConversationSummary } from "./types"

export const DEFAULT_USAGE_SESSION_LIMIT = 60
export const DEFAULT_USAGE_LOAD_CONCURRENCY = 3

export type UsageDetailLoader = (
  conversation: ConversationSummary
) => Promise<ConversationDetail>

export interface UsageDetailsLoadOptions {
  concurrency?: number
  isCurrent?: () => boolean
  loadConversation: UsageDetailLoader
}

export interface UsageDetailsLoadResult {
  details: ConversationDetail[]
  failedConversations: number
}

function resolveConcurrency(value: number | undefined): number {
  if (value === undefined) return DEFAULT_USAGE_LOAD_CONCURRENCY
  if (!Number.isFinite(value)) return DEFAULT_USAGE_LOAD_CONCURRENCY
  return Math.max(1, Math.floor(value))
}

export async function loadUsageDetails(
  conversations: ConversationSummary[],
  options: UsageDetailsLoadOptions
): Promise<UsageDetailsLoadResult> {
  const concurrency = resolveConcurrency(options.concurrency)
  const isCurrent = options.isCurrent ?? (() => true)
  const details: ConversationDetail[] = []
  let failedConversations = 0

  for (
    let index = 0;
    index < conversations.length && isCurrent();
    index += concurrency
  ) {
    const batch = conversations.slice(index, index + concurrency)
    const settled = await Promise.allSettled(
      batch.map((conversation) => options.loadConversation(conversation))
    )

    if (!isCurrent()) break

    for (const result of settled) {
      if (result.status === "fulfilled") {
        details.push(result.value)
      } else {
        failedConversations += 1
      }
    }
  }

  return { details, failedConversations }
}
