"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { createFolderDirectory, listDirectoryWithFiles } from "@/lib/api"
import { joinFsPath } from "@/lib/path-utils"
import { toErrorMessage } from "@/lib/app-error"
import { getTransport } from "@/lib/transport"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"

function validProjectName(name: string) {
  return (
    !!name &&
    !name.startsWith(".") &&
    !/[<>:"/\\|?*\u0000-\u001f]/.test(name) &&
    !/[. ]$/.test(name) &&
    !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)
  )
}

export function useNewProject(onCreated: () => void) {
  const t = useTranslations("SidebarDesign")
  const [name, setName] = useState("")
  const [parent, setParent] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [pending, setPending] = useState(false)
  const busy = useRef(false)
  const createdPath = useRef<string | null>(null)
  const openFolder = useAppWorkspaceStore((s) => s.openFolder)
  const { openNewConversationTab } = useTabActions()
  const { openConversations } = useWorkbenchRoute()
  const showError = (message: string) => {
    setError(message)
    toast.error(message)
  }
  const create = async () => {
    if (busy.current) return
    const trimmed = name.trim()
    if (!validProjectName(trimmed)) {
      showError(t("invalidName"))
      return
    }
    if (!parent) {
      showError(t("chooseParent"))
      return
    }
    busy.current = true
    setPending(true)
    setError(null)
    const backend = getTransport()
    try {
      const path = joinFsPath(parent, trimmed)
      if (createdPath.current !== path) {
        const entries = await listDirectoryWithFiles(parent)
        if (
          entries.some(
            (entry) =>
              entry.name.toLocaleLowerCase() === trimmed.toLocaleLowerCase()
          )
        )
          throw new Error(t("projectExists"))
        if (backend !== getTransport()) return
        await createFolderDirectory(path)
        createdPath.current = path
      }
      if (backend !== getTransport()) return
      const folder = await openFolder(path)
      if (backend !== getTransport()) return
      openConversations()
      const selected = await openNewConversationTab(folder.id, folder.path)
      if (backend !== getTransport()) return
      if (!selected) {
        console.warn("[sidebar-project] created but activation blocked", {
          folderId: folder.id,
        })
        showError(t("projectOpenFailed"))
        return
      }
      console.info("[sidebar-project] created and opened", {
        folderId: folder.id,
      })
      onCreated()
      toast.success(t("projectCreated"))
    } catch (reason) {
      const message = toErrorMessage(reason)
      console.error("[sidebar-project] create or open failed", { message })
      showError(t("projectFailed", { message }))
    } finally {
      busy.current = false
      setPending(false)
    }
  }
  return { name, setName, parent, setParent, error, pending, create }
}
