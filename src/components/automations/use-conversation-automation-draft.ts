"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useAutomationsView } from "@/contexts/automations-view-context"
import { automationDraftFromConversation } from "@/lib/api"
import type { AutomationDraft } from "@/lib/types"

function detectTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC"
  } catch {
    return "UTC"
  }
}

export function useConversationAutomationDraft(
  onReady: (draft: AutomationDraft) => void
) {
  const t = useTranslations("Automations")
  const { createRequest, clearCreateRequest } = useAutomationsView()

  useEffect(() => {
    if (!createRequest) return
    const request = createRequest
    const toastId = `automation-draft-${request.nonce}`
    let cancelled = false
    let finished = false
    toast.loading(t("drafting"), { id: toastId })
    void automationDraftFromConversation(request.conversationId, {
      timeoutMs: 210_000,
    })
      .then((source) => {
        if (cancelled) return
        const prompt = source.prompt
        onReady({
          name: source.name.trim() || t("new"),
          enabled: true,
          trigger_kind: "schedule",
          cron: "0 9 * * 1-5",
          timezone: detectTimezone(),
          agent_type: source.agentType,
          root_folder_id: source.rootFolderId,
          isolation: "shared_in_root",
          branch: null,
          is_remote_branch: false,
          config: {
            prompt_blocks: [{ type: "text", text: prompt }],
            display_text: prompt,
            config_values: {},
          },
        })
        finished = true
        toast.dismiss(toastId)
      })
      .catch((error) => {
        if (cancelled) return
        finished = true
        toast.error(error instanceof Error ? error.message : String(error), {
          id: toastId,
        })
      })
      .finally(() => clearCreateRequest(request.nonce))
    return () => {
      cancelled = true
      if (!finished) toast.dismiss(toastId)
      clearCreateRequest(request.nonce)
    }
  }, [clearCreateRequest, createRequest, onReady, t])
}
