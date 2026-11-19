import { getTransport } from "@/lib/transport"
import { createChatConversation, createConversation } from "@/lib/api"
import type { AgentType } from "@/lib/types"
import { useTabStore } from "@/contexts/tab-context"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"

export interface ConversationConnectorChoice {
  id: string
  name: string
  protocol: string
  compatible: boolean
  reason: string | null
  requires_migration: boolean
}

export interface ConversationConnectors {
  revision: number
  selected: string[]
  connectors: ConversationConnectorChoice[]
  reconnect_required: boolean
  migrated: string[]
}

const CONNECTORS_CHANGED = "iyw-claw:conversation-connectors-changed"

export function onConversationConnectorsChanged(
  conversationId: number,
  listener: (data: ConversationConnectors) => void
) {
  const handle = (event: Event) => {
    const update = (
      event as CustomEvent<{
        conversationId: number
        data: ConversationConnectors
      }>
    ).detail
    if (update?.conversationId === conversationId) listener(update.data)
  }
  window.addEventListener(CONNECTORS_CHANGED, handle)
  return () => window.removeEventListener(CONNECTORS_CHANGED, handle)
}

export function getConversationConnectors(
  conversationId: number | null,
  agentType: AgentType | null
) {
  return getTransport().call<ConversationConnectors>("mcp_conversation_get", {
    conversationId,
    agentType,
  })
}

export async function prepareConnectorConversation(
  tabId: string,
  title: string
) {
  const tab = useTabStore.getState().tabs.find((item) => item.id === tabId)
  if (!tab) throw new Error("Conversation tab unavailable")
  if (tab.conversationId != null)
    return { id: tab.conversationId, bind: () => {} }
  const created = tab.isChat
    ? await createChatConversation(tab.agentType, title, tab.workingDir)
    : null
  const id =
    created?.conversationId ??
    (await createConversation(tab.folderId, tab.agentType, title))
  return {
    id,
    bind: () => {
      if (!useTabStore.getState().rawTabs.some((item) => item.id === tabId))
        return
      if (created) useAppWorkspaceStore.getState().upsertFolder(created.folder)
      useTabStore
        .getState()
        .bindConversationTab(
          tabId,
          id,
          tab.agentType,
          title,
          undefined,
          created?.folderId,
          created?.folder.path
        )
      void useAppWorkspaceStore.getState().refreshConversations()
    },
  }
}

export async function setConversationConnectors(input: {
  conversationId: number
  revision: number
  selected: string[]
}) {
  const transport = getTransport()
  const data = await transport.call<ConversationConnectors>(
    "mcp_conversation_set",
    {
      input,
    }
  )
  if (typeof window !== "undefined" && transport === getTransport())
    window.dispatchEvent(
      new CustomEvent(CONNECTORS_CHANGED, {
        detail: { conversationId: input.conversationId, data },
      })
    )
  return data
}
