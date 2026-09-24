import { useTranslations } from "next-intl"
import type { ExportLabels } from "@/lib/export-conversation"

export function useConversationExportLabels(): ExportLabels {
  const t = useTranslations("Folder.conversation.exportLabels")
  const status = useTranslations("Folder.statusLabels")
  return {
    untitledConversation: t("untitledConversation"),
    agent: t("agent"),
    model: t("model"),
    status: t("status"),
    started: t("started"),
    updated: t("updated"),
    tokens: t("tokens"),
    duration: t("duration"),
    inputTokens: t("inputTokens"),
    outputTokens: t("outputTokens"),
    cacheRead: t("cacheRead"),
    cacheWrite: t("cacheWrite"),
    user: t("user"),
    assistant: t("assistant"),
    system: t("system"),
    toolResult: t("toolResult"),
    toolError: t("toolError"),
    statusLabels: {
      in_progress: status("in_progress"),
      pending_review: status("pending_review"),
      completed: status("completed"),
      cancelled: status("cancelled"),
    },
  }
}
