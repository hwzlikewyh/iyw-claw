"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { updateChatChannel } from "@/lib/api"
import { parseChannelConfig } from "@/lib/chat-channel-setup"
import type { ChatChannelInfo } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import {
  ChannelFinalizeForm,
  type ChannelFinalizeValues,
} from "./channel-finalize-form"

export function ChannelFinalizeDialog({
  channel,
  open,
  onOpenChange,
  onComplete,
}: {
  channel: ChatChannelInfo
  open: boolean
  onOpenChange: (open: boolean) => void
  onComplete: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const config = parseChannelConfig(channel)

  const submit = async (values: ChannelFinalizeValues) => {
    setSubmitting(true)
    setError(null)
    try {
      const updated = await updateChatChannel({
        id: channel.id,
        name: values.name,
        enabled: true,
        configPatchJson: JSON.stringify({
          setupState: "ready",
          defaultAgentType: values.defaultAgentType,
        }),
        dailyReportEnabled: values.dailyReportEnabled,
        dailyReportTime: values.dailyReportEnabled
          ? values.dailyReportTime
          : null,
      })
      if (updated.runtime_status === "error") {
        setError(updated.last_error ?? t("savedButConnectFailed"))
        return
      }
      onOpenChange(false)
      onComplete()
    } catch (caught) {
      setError(toErrorMessage(caught))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("market.finishSetup")}</DialogTitle>
        </DialogHeader>
        <ChannelFinalizeForm
          channelType={channel.channel_type}
          initialName={channel.name}
          initialDefaultAgentType={config.default_agent_type ?? null}
          submitting={submitting}
          error={error}
          onCancel={() => onOpenChange(false)}
          onSubmit={submit}
        />
      </DialogContent>
    </Dialog>
  )
}
