"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { copyTextFromMenu } from "@/lib/utils"
import { formatConversationTitle } from "@/lib/conversation-title"
import type { DbConversationSummary } from "@/lib/types"
import { loadConversationMenuHistory } from "./conversation-menu-data"
import { useConversationExportLabels } from "./conversation-menu-labels"

export type ConversationMenuAction =
  | "copyTitle"
  | "copyContent"
  | "markdown"
  | "html"

const SUCCESS_LABELS = {
  copyTitle: "titleCopied",
  copyContent: "contentCopied",
  markdown: "exported",
  html: "exported",
} as const

export function useConversationMenuActions(summary: DbConversationSummary) {
  const t = useTranslations("Folder.conversationMenu")
  const labels = useConversationExportLabels()
  const [busy, setBusy] = useState(false)
  const active = useRef<AbortController | null>(null)
  const pendingToast = useRef<string | number | undefined>(undefined)
  useEffect(
    () => () => {
      active.current?.abort()
      if (pendingToast.current !== undefined)
        toast.dismiss(pendingToast.current)
    },
    []
  )

  const run = async (action: ConversationMenuAction) => {
    if (active.current) return
    const controller = new AbortController()
    active.current = controller
    setBusy(true)
    const toastId =
      action === "copyTitle" ? undefined : toast.loading(t("loading"))
    pendingToast.current = toastId
    try {
      await performAction({
        summary,
        action,
        labels,
        signal: controller.signal,
      })
      if (toastId !== undefined) toast.dismiss(toastId)
      if (!controller.signal.aborted) toast.success(t(SUCCESS_LABELS[action]))
    } catch (error) {
      if (toastId !== undefined) toast.dismiss(toastId)
      if (!controller.signal.aborted && error !== EXPORT_CANCELLED) {
        console.error("[conversation-menu] action failed", {
          conversationId: summary.id,
          action,
          error,
        })
        toast.error(t("actionFailed"))
      }
    } finally {
      active.current = null
      pendingToast.current = undefined
      if (!controller.signal.aborted) setBusy(false)
    }
  }
  return { busy, run }
}

const EXPORT_CANCELLED = Symbol("export-cancelled")

async function performAction(options: {
  summary: DbConversationSummary
  action: ConversationMenuAction
  labels: ReturnType<typeof useConversationExportLabels>
  signal: AbortSignal
}) {
  const { summary, action, labels, signal } = options
  if (action === "copyTitle") {
    const title =
      formatConversationTitle(summary.title) || labels.untitledConversation
    if (!(await copyTextFromMenu(title)))
      throw new Error("Clipboard write failed")
    return
  }
  const detail = await loadConversationMenuHistory(summary.id, signal)
  const exports = await import("@/lib/export-conversation")
  signal.throwIfAborted()
  const data = {
    summary: { ...detail.summary, title: summary.title },
    turns: detail.turns,
    sessionStats: detail.session_stats,
    labels,
  }
  if (action === "copyContent") {
    if (!(await copyTextFromMenu(exports.formatConversationMarkdown(data)))) {
      throw new Error("Clipboard write failed")
    }
    return
  }
  const result = await (action === "markdown"
    ? exports.exportAsMarkdown(data)
    : exports.exportAsHtml(data))
  if (result === "cancelled") throw EXPORT_CANCELLED
}
