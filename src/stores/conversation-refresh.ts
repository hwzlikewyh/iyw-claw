import type { DbConversationSummary } from "@/lib/types"

type ConversationPatch = Partial<
  Pick<DbConversationSummary, "status" | "title" | "pinned_at" | "updated_at">
>

// 记录请求期间的事件，避免较晚返回的列表覆盖实时状态。
export function createConversationRefresh() {
  const upserts = new Map<number, DbConversationSummary>()
  const patches = new Map<number, ConversationPatch>()

  return {
    patch(id: number, patch: ConversationPatch) {
      patches.set(id, { ...patches.get(id), ...patch })
    },
    upsert(summary: DbConversationSummary) {
      upserts.set(summary.id, summary)
      patches.delete(summary.id)
    },
    merge(list: DbConversationSummary[], deletedIds: ReadonlySet<number>) {
      const rows = new Map(list.map((summary) => [summary.id, summary]))
      for (const [id, summary] of upserts) rows.set(id, summary)
      const result: DbConversationSummary[] = []
      for (const [id, summary] of rows) {
        if (deletedIds.has(id)) continue
        const patch = patches.get(id)
        result.push(patch ? { ...summary, ...patch } : summary)
      }
      return result
    },
  }
}
