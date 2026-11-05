"use client"

import { useRef } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { deleteTaskArtifacts, type TaskArtifactInfo } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"

export function useDeleteTaskArtifacts(onDeleted: () => Promise<void>) {
  const t = useTranslations("Folder.taskArtifacts")
  const busy = useRef(false)
  return async (items: TaskArtifactInfo[]) => {
    if (!items.length || busy.current) return
    if (!window.confirm(t("deleteConfirmation", { count: items.length })))
      return
    if (new Set(items.map((item) => item.conversationId)).size !== 1) {
      toast.error(t("deleteSameConversation"))
      return
    }
    busy.current = true
    try {
      await deleteTaskArtifacts(
        items[0].conversationId,
        items.map((item) => item.id)
      )
      await onDeleted()
      toast.success(t("deleted"))
    } catch (error) {
      console.error("[TaskArtifacts] deletion failed", {
        count: items.length,
        error: toErrorMessage(error),
      })
      toast.error(t("deleteFailed"))
    } finally {
      busy.current = false
    }
  }
}
