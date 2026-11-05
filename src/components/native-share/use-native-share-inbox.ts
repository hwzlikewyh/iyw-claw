"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useTabStore } from "@/stores/tab-store"
import { useSortedAvailableAgents } from "@/hooks/use-sorted-available-agents"
import { deliverNativeShare } from "./native-share-delivery"
import type { AgentType } from "@/lib/types"
import {
  importNativeShare,
  nativeShareStatus,
  registerNativeShare,
  shareError,
} from "@/lib/native-share"
import {
  useShareState,
  useShareRefresh,
  useShareEvents,
  useSharePreview,
  useShareStatus,
  type ShareInboxState,
  type PatchShareState,
} from "./use-native-share-data"

type Translate = ReturnType<typeof useTranslations<"NativeShare">>

function useBusyOperation(patch: PatchShareState) {
  const running = useRef(false)
  return useCallback(
    async (operation: () => Promise<void>) => {
      if (running.current) return false
      running.current = true
      patch({ busy: true, error: null })
      try {
        await operation()
      } catch (error) {
        patch({ error: shareError(error), open: true })
      } finally {
        running.current = false
        patch({ busy: false })
      }
      return true
    },
    [patch]
  )
}

async function pickArchive(t: Translate): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog")
  const paths = await open({
    multiple: true,
    filters: [{ name: t("archiveFiles"), extensions: ["zip", "txt"] }],
  })
  if (!paths) return null
  return (await importNativeShare(Array.isArray(paths) ? paths : [paths])).id
}

function previewReload(previous: ShareInboxState) {
  return {
    loading: previous.selected !== null,
    preview: null,
    error: null,
    previewRevision: previous.previewRevision + 1,
  }
}

function selectBatch(previous: ShareInboxState, selected: string) {
  return { ...previewReload(previous), selected, loading: true }
}

function useShareCommands(
  state: ShareInboxState,
  patch: PatchShareState,
  context: { t: Translate; refresh: () => Promise<void>; agentType?: AgentType }
) {
  const { t, refresh } = context
  const run = useBusyOperation(patch)
  const manualImport = () =>
    run(async () => {
      const id = await pickArchive(t)
      if (id) {
        await refresh()
        patch((previous) => selectBatch(previous, id))
      }
    })
  const addToDraft = (batchId = state.selected) =>
    run(async () => {
      if (!batchId || !context.agentType)
        throw new Error(t("composerUnavailable"))
      patch((previous) => selectBatch(previous, batchId))
      await deliverNativeShare({
        batchId,
        agentType: context.agentType,
        title: t("conversationTitle"),
        prompt: t("prompt"),
        remoteUnsupported: t("remoteUnsupported"),
      })
      await refresh()
      patch({ open: false })
      toast.success(t("added"))
    })
  return {
    manualImport,
    addToDraft,
    register: () =>
      run(async () => {
        await registerNativeShare()
        patch({ status: await nativeShareStatus() })
        toast.success(t("registrationComplete"))
      }),
    setOpen: (open: boolean) => patch({ open }),
    setSelected: (selected: string) =>
      patch((previous) => selectBatch(previous, selected)),
    refresh: () =>
      run(async () => {
        await refresh()
        patch(previewReload)
      }),
  }
}

export function useNativeShareInbox() {
  const t = useTranslations("NativeShare")
  const { sortedTypes, fresh } = useSortedAvailableAgents()
  const tabsHydrated = useTabStore((store) => store.tabsHydrated)
  const activeAgent = useTabStore(
    (store) =>
      store.rawTabs.find((tab) => tab.id === store.activeTabId)?.agentType
  )
  const { state, patch } = useShareState()
  const refresh = useShareRefresh(patch)
  useShareEvents(refresh, patch)
  useSharePreview(state, patch)
  useShareStatus(state.open, patch)
  const commands = useShareCommands(state, patch, {
    t,
    refresh,
    agentType:
      activeAgent && sortedTypes.includes(activeAgent)
        ? activeAgent
        : sortedTypes[0],
  })
  useAutomaticDelivery(state, patch, {
    ready: fresh && tabsHydrated,
    hasAgent: sortedTypes.length > 0,
    addToDraft: commands.addToDraft,
    noAgent: t("noAvailableAgent"),
  })
  return { ...state, t, ...commands }
}

function useAutomaticDelivery(
  state: ShareInboxState,
  patch: PatchShareState,
  context: {
    ready: boolean
    hasAgent: boolean
    addToDraft: (id: string) => Promise<boolean>
    noAgent: string
  }
) {
  // 完成记录使用 state，确保上一批结束后会重新检查等待中的批次。
  const [attempted, setAttempted] = useState(() => new Set<string>())
  const delivering = useRef(false)
  useEffect(() => {
    if (delivering.current || !context.ready || state.busy || state.open) return
    const pending = state.items.find(
      (item) => !item.imported && !attempted.has(item.id)
    )
    if (!pending) return
    if (!context.hasAgent) {
      patch({ open: true, selected: pending.id, error: context.noAgent })
      return
    }
    delivering.current = true
    void context.addToDraft(pending.id).then(
      (didRun) => {
        delivering.current = false
        if (didRun)
          setAttempted((previous) => new Set(previous).add(pending.id))
      },
      (error) => {
        delivering.current = false
        patch({ open: true, error: shareError(error) })
      }
    )
  }, [attempted, context, state.busy, state.items, state.open, patch])
}
