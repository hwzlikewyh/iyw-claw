"use client"

import { useCallback } from "react"
import { flushSync } from "react-dom"

import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import type { TaskArtifactInfo } from "@/lib/api"
import { emitAttachFileToSession } from "@/lib/session-attachment-events"
import { useTabStore } from "@/stores/tab-store"

export function useSendArtifactToChat({
  artifact,
  onOpenWorkspace,
}: {
  artifact: TaskArtifactInfo
  onOpenWorkspace?: () => void
}): (() => void) | undefined {
  const tabId = useTabStore(
    (state) => state.tabs.find((tab) => tab.id === state.activeTabId)?.id
  )
  const { openConversations } = useWorkbenchRoute()
  const { activateConversationPane } = useWorkspaceActions()
  const available = artifact.status === "available" && artifact.kind !== "url"
  const sendToChat = useCallback(() => {
    if (!tabId || !available) return
    flushSync(() => {
      onOpenWorkspace?.()
      openConversations()
      activateConversationPane()
    })
    // 先关闭弹窗并解除对话区的 inert，再让输入框恢复光标和焦点。
    requestAnimationFrame(() => {
      const state = useTabStore.getState()
      if (!state.tabs.some((tab) => tab.id === tabId)) return
      emitAttachFileToSession({ tabId, path: artifact.path })
      console.info("[task-artifacts] sent file to composer", {
        artifactId: artifact.id,
        artifactKind: artifact.kind,
        tabId,
      })
    })
  }, [
    activateConversationPane,
    artifact.id,
    artifact.kind,
    artifact.path,
    available,
    onOpenWorkspace,
    openConversations,
    tabId,
  ])
  return tabId && available ? sendToChat : undefined
}
