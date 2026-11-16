"use client"

import { useTabStore } from "@/contexts/tab-context"

// 插件准备仅复用已发送的会话，不为草稿创建持久化记录。
export function usePluginWorkspace(tabId: string | null) {
  const bind = async () =>
    useTabStore.getState().tabs.find((tab) => tab.id === tabId)
      ?.conversationId ?? null
  return { busy: false, error: "", bind }
}
