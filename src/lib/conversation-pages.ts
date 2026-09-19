import { listAllConversations } from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"
import type { AgentType, DbConversationSummary } from "@/lib/types"

export interface ConversationCursor {
  at: string
  id: number
}

export interface ConversationPage {
  items: DbConversationSummary[]
  next_cursor: ConversationCursor | null
  incomplete?: boolean
}

export interface ConversationPageRequest {
  folderIds?: number[] | null
  agentType?: AgentType | null
  search?: string | null
  status?: string | null
  sortBy?: string | null
  includeChildren?: boolean
  cursor?: ConversationCursor | null
  pageSize?: number
}

export async function listConversationsPage(
  params: ConversationPageRequest
): Promise<ConversationPage> {
  try {
    return await getTransport().call("list_conversations_page", { params })
  } catch (error) {
    const message =
      extractAppCommandError(error)?.message ?? toErrorMessage(error)
    const unsupported =
      /^(?:HTTP 404|Remote returned HTTP 404(?: Not Found)?)$/i.test(message) ||
      /^Command ["']?list_conversations_page["']? not found$/i.test(message)
    if (!unsupported) throw error
    // 旧服务端仍按原接口完整返回；其他错误保留原错误语义。
    const items = await listAllConversations({
      folder_ids: params.folderIds,
      agent_type: params.agentType,
      search: params.search,
      status: params.status,
      sort_by: params.sortBy,
      include_children: params.includeChildren,
    })
    return { items, next_cursor: null }
  }
}
