"use client"

import { useCallback, useRef } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useTabStore } from "@/stores/tab-store"
import { isRemoteDesktopMode } from "@/lib/transport"
import { requireFileUploadCapability } from "@/lib/transport/capability-policy"
import {
  addNativeShareToDraft,
  completeNativeShare,
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
      if (running.current) return
      running.current = true
      patch({ busy: true, error: null })
      try {
        await operation()
      } catch (error) {
        patch({ error: shareError(error) })
      } finally {
        running.current = false
        patch({ busy: false })
      }
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

async function deliverDraft(
  state: ShareInboxState,
  t: Translate
): Promise<void> {
  if (isRemoteDesktopMode()) throw new Error(t("remoteUnsupported"))
  const { preview, selected, targetTab } = state
  if (!preview || preview.id !== selected || !targetTab)
    throw new Error(t("chooseConversation"))
  await requireFileUploadCapability()
  const store = useTabStore.getState()
  if (!store.rawTabs.some((tab) => tab.id === targetTab))
    throw new Error(t("chooseConversation"))
  store.switchTab(targetTab)
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  await addNativeShareToDraft(
    {
      tabId: targetTab,
      batchId: preview.id,
      paths: preview.paths,
      prompt: t("prompt"),
    },
    t("composerUnavailable")
  )
  await completeNativeShare(preview.id)
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
  context: { t: Translate; refresh: () => Promise<void> }
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
  const addToDraft = () =>
    run(async () => {
      await deliverDraft(state, t)
      await refresh()
      patch({ open: false })
      toast.success(t("added"))
    })
  const register = () =>
    run(async () => {
      await registerNativeShare()
      patch({ status: await nativeShareStatus() })
      toast.success(t("registrationComplete"))
    })
  return {
    manualImport,
    addToDraft,
    register,
    setOpen: (open: boolean) => patch({ open }),
    setSelected: (selected: string) =>
      patch((previous) => selectBatch(previous, selected)),
    setTargetTab: (targetTab: string) => patch({ targetTab }),
    refresh: () =>
      run(async () => {
        await refresh()
        patch(previewReload)
      }),
  }
}

export function useNativeShareInbox() {
  const t = useTranslations("NativeShare")
  const tabs = useTabStore((state) => state.rawTabs)
  const { state, patch } = useShareState()
  const refresh = useShareRefresh(patch)
  useShareEvents(refresh, patch)
  useSharePreview(state, patch)
  useShareStatus(state.open, patch)
  const commands = useShareCommands(state, patch, { t, refresh })
  return { ...state, t, tabs, ...commands }
}
