"use client"

import { useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ChatChannelDailyReportFields } from "./chat-channel-daily-report-fields"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import type { AgentType, ChannelType } from "@/lib/types"

const NO_AGENT = "__none__"

export interface ChannelFinalizeValues {
  name: string
  defaultAgentType: AgentType | null
  defaultUserId: string
  dailyReportEnabled: boolean
  dailyReportTime: string
}

export function ChannelFinalizeForm({
  channelType,
  initialName,
  initialDefaultAgentType = null,
  initialDefaultUserId = "",
  initialDailyReportEnabled = false,
  initialDailyReportTime = "18:00",
  showDefaultUserId = false,
  submitting,
  error,
  onCancel,
  onSubmit,
}: {
  channelType: ChannelType
  initialName: string
  initialDefaultAgentType?: AgentType | null
  initialDefaultUserId?: string
  initialDailyReportEnabled?: boolean
  initialDailyReportTime?: string
  showDefaultUserId?: boolean
  submitting: boolean
  error: string | null
  onCancel: () => void
  onSubmit: (values: ChannelFinalizeValues) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const { agents } = useAcpAgents()
  const [name, setName] = useState(initialName)
  const [defaultAgentType, setDefaultAgentType] = useState<AgentType | null>(
    initialDefaultAgentType
  )
  const [defaultUserId, setDefaultUserId] = useState(initialDefaultUserId)
  const [dailyReportEnabled, setDailyReportEnabled] = useState(
    initialDailyReportEnabled
  )
  const [dailyReportTime, setDailyReportTime] = useState(initialDailyReportTime)
  const installedAgents = agents.filter(
    (agent) => agent.enabled && agent.installed_version
  )

  const submit = () => {
    if (!name.trim()) return
    onSubmit({
      name: name.trim(),
      defaultAgentType,
      defaultUserId: defaultUserId.trim(),
      dailyReportEnabled,
      dailyReportTime,
    })
  }

  return (
    <div className="min-w-0 space-y-4">
      <div className="space-y-1.5">
        <label className="text-xs font-medium">{t("channelName")}</label>
        <Input value={name} onChange={(event) => setName(event.target.value)} />
      </div>
      <div className="space-y-1.5">
        <label className="text-xs font-medium">{t("defaultAgent")}</label>
        <Select
          value={defaultAgentType ?? NO_AGENT}
          onValueChange={(value) =>
            setDefaultAgentType(
              value === NO_AGENT ? null : (value as AgentType)
            )
          }
        >
          <SelectTrigger className="w-full">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NO_AGENT}>{t("defaultAgentNone")}</SelectItem>
            {installedAgents.map((agent) => (
              <SelectItem key={agent.agent_type} value={agent.agent_type}>
                {agent.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {showDefaultUserId && (
        <div className="space-y-1.5">
          <label className="text-xs font-medium">
            {t("market.defaultUserId")}
          </label>
          <Input
            value={defaultUserId}
            onChange={(event) => setDefaultUserId(event.target.value)}
            placeholder={t("market.defaultUserIdPlaceholder")}
          />
          <p className="text-xs text-muted-foreground">
            {t("market.defaultUserIdHint")}
          </p>
        </div>
      )}
      <ChatChannelDailyReportFields
        channelType={channelType}
        enabled={dailyReportEnabled}
        time={dailyReportTime}
        onEnabledChange={setDailyReportEnabled}
        onTimeChange={setDailyReportTime}
      />
      {error && (
        <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {error}
        </div>
      )}
      <div className="flex flex-col-reverse gap-2 pt-2 sm:flex-row sm:justify-end">
        <Button variant="outline" onClick={onCancel} disabled={submitting}>
          {t("market.keepDraft")}
        </Button>
        <Button onClick={submit} disabled={submitting || !name.trim()}>
          {submitting && <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />}
          {t("market.enableChannel")}
        </Button>
      </div>
    </div>
  )
}
