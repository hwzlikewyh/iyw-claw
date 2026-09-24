import {
  deleteConversation,
  updateConversationPinned,
  updateConversationStatus,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import type { ConversationStatus, DbConversationSummary } from "@/lib/types"

export type ManagerAction =
  | "pin"
  | "unpin"
  | "complete"
  | "delete"
  | ConversationStatus
export interface ManagerActionResult {
  id: number
  error?: string
}

export async function runConversationBatch(options: {
  action: ManagerAction
  rows: DbConversationSummary[]
  signal: AbortSignal
  onDeleted: (row: DbConversationSummary) => void
}) {
  const backend = getTransport()
  const results: ManagerActionResult[] = []
  for (const row of options.rows) {
    if (options.signal.aborted || backend !== getTransport()) return null
    try {
      await applyManagerAction(options.action, row, backend)
      if (options.signal.aborted || backend !== getTransport()) return null
      if (options.action === "delete") options.onDeleted(row)
      results.push({ id: row.id })
    } catch (reason) {
      results.push({ id: row.id, error: toErrorMessage(reason) })
    }
  }
  return results
}

async function applyManagerAction(
  action: ManagerAction,
  row: DbConversationSummary,
  backend: ReturnType<typeof getTransport>
) {
  if (action === "delete") {
    await deleteConversation(row.id)
    if (backend === getTransport())
      useAppWorkspaceStore.getState().applyConversationRemove(row.id)
    return
  }
  const store = useAppWorkspaceStore.getState()
  if (action !== "pin" && action !== "unpin") {
    const status = action === "complete" ? "completed" : action
    await updateConversationStatus(row.id, status)
    if (backend === getTransport())
      store.updateConversationLocal(row.id, { status })
  } else {
    const pinned = action === "pin"
    await updateConversationPinned(row.id, pinned)
    if (backend === getTransport())
      store.updateConversationLocal(row.id, {
        pinned_at: pinned ? new Date().toISOString() : null,
      })
  }
}
