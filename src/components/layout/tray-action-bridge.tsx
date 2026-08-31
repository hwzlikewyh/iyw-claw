"use client"

import { useCallback, useEffect, useMemo } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { useActiveFolder } from "@/contexts/active-folder-context"
import { useTabActions, useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { useAppUpdate } from "@/components/providers/update-provider"
import { openSettingsWindow } from "@/lib/api"
import {
  TRAY_ACTION_EVENT,
  refreshTrayMenu,
  type TrayActionPayload,
} from "@/lib/tauri"
import {
  getShellTransport,
  isDesktop,
  isRemoteDesktopMode,
} from "@/lib/transport"
import { checkAppUpdate, normalizeAppUpdateError } from "@/lib/updater"
import { toErrorMessage } from "@/lib/app-error"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"

function isTrayActionPayload(value: unknown): value is TrayActionPayload {
  if (!value || typeof value !== "object") return false
  const action = (value as { action?: unknown }).action
  if (
    action !== "open" &&
    action !== "new" &&
    action !== "settings" &&
    action !== "update" &&
    action !== "recent"
  ) {
    return false
  }
  if (action !== "recent") return true
  return (
    typeof (value as { folderId?: unknown }).folderId === "number" &&
    Number.isInteger((value as { folderId: number }).folderId)
  )
}

export function TrayActionBridge() {
  const t = useTranslations("SystemSettings")
  const { activeFolder } = useActiveFolder()
  const { openNewConversationTab, openChatModeTab, switchTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const update = useAppUpdate()
  const allFolders = useAppWorkspaceStore((state) => state.allFolders)
  const foldersHydrated = useAppWorkspaceStore((state) => state.foldersHydrated)
  const addFolderToWorkspaceById = useAppWorkspaceStore(
    (state) => state.addFolderToWorkspaceById
  )
  const folderSignature = useMemo(
    () =>
      allFolders
        .map((folder) => `${folder.id}:${folder.name}:${folder.last_opened_at}`)
        .join("|"),
    [allFolders]
  )

  const handleAction = useCallback(
    async (payload: TrayActionPayload) => {
      if (payload.action === "open") {
        openConversations()
        return
      }

      if (payload.action === "new") {
        openConversations()
        if (activeFolder) {
          openNewConversationTab(activeFolder.id, activeFolder.path)
        } else {
          openChatModeTab()
        }
        return
      }

      if (payload.action === "settings") {
        try {
          await openSettingsWindow("appearance")
        } catch (error) {
          toast.error(toErrorMessage(error))
          console.warn("[Tray] failed to open settings:", error)
        }
        return
      }

      if (payload.action === "update") {
        if (update?.isBusy) return
        try {
          const result = await checkAppUpdate()
          if (!result.update) toast.success(t("alreadyLatest"))
        } catch (error) {
          const { rawMessage } = normalizeAppUpdateError(error)
          toast.error(t("checkUpdateFailed", { message: rawMessage }))
          console.warn("[Tray] update check failed:", error)
        }
        return
      }

      openConversations()
      try {
        const folder = await addFolderToWorkspaceById(payload.folderId)
        const existingTab = useTabStore
          .getState()
          .rawTabs.find((tab) => tab.folderId === folder.id && !tab.isChat)
        if (existingTab) {
          switchTab(existingTab.id)
        } else {
          openNewConversationTab(folder.id, folder.path)
        }
      } catch (error) {
        toast.error(toErrorMessage(error))
        console.warn("[Tray] failed to open recent project:", error)
      }
    },
    [
      activeFolder,
      addFolderToWorkspaceById,
      openChatModeTab,
      openConversations,
      openNewConversationTab,
      switchTab,
      t,
      update?.isBusy,
    ]
  )

  useEffect(() => {
    if (!isDesktop() || isRemoteDesktopMode()) return
    let disposed = false
    let unsubscribe: (() => void) | undefined

    void getShellTransport()
      .subscribe<unknown>(TRAY_ACTION_EVENT, (payload) => {
        if (!disposed && isTrayActionPayload(payload))
          void handleAction(payload)
      })
      .then((dispose) => {
        if (disposed) dispose()
        else unsubscribe = dispose
      })
      .catch((error) => {
        console.warn("[Tray] action subscription failed:", error)
      })

    return () => {
      disposed = true
      unsubscribe?.()
    }
  }, [handleAction])

  useEffect(() => {
    if (!isDesktop() || isRemoteDesktopMode() || !foldersHydrated) return
    void refreshTrayMenu().catch((error) => {
      console.warn("[Tray] recent project refresh failed:", error)
    })
  }, [folderSignature, foldersHydrated])

  return null
}
