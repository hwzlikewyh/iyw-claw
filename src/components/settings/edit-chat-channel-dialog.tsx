"use client"

import { useCallback, useEffect, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ChatChannelCredentialFields } from "@/components/settings/chat-channel-credential-fields"
import { ChatChannelDailyReportFields } from "@/components/settings/chat-channel-daily-report-fields"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import {
  updateChatChannel,
  saveChatChannelToken,
  getChatChannelHasToken,
  listChatChannels,
} from "@/lib/api"
import type { AgentType, ChatChannelInfo } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import { buildChatChannelConfigPatch } from "@/lib/chat-channel-config"

interface EditChatChannelDialogProps {
  open: boolean
  channel: ChatChannelInfo
  onOpenChange: (open: boolean) => void
  onChannelUpdated: () => void
}

const NO_AGENT = "__none__"

export function EditChatChannelDialog({
  open,
  channel,
  onOpenChange,
  onChannelUpdated,
}: EditChatChannelDialogProps) {
  const t = useTranslations("ChatChannelSettings")
  const { agents } = useAcpAgents()
  const installedAgents = agents.filter(
    (agent) => agent.enabled && agent.installed_version
  )
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const config = JSON.parse(channel.config_json || "{}")
  const [name, setName] = useState(channel.name)
  const [token, setToken] = useState("")
  const [chatId, setChatId] = useState(
    config.chat_id ?? config.default_chatid ?? ""
  )
  const [appId, setAppId] = useState(config.app_id ?? "")
  const [botId, setBotId] = useState(config.bot_id ?? "")
  const [clientId, setClientId] = useState(config.client_id ?? "")
  const [baseUrl] = useState(config.base_url ?? "")
  const [defaultAgentType, setDefaultAgentType] = useState<AgentType | null>(
    config.default_agent_type ?? null
  )
  const [dailyReportEnabled, setDailyReportEnabled] = useState(
    channel.daily_report_enabled
  )
  const [dailyReportTime, setDailyReportTime] = useState(
    channel.daily_report_time || "18:00"
  )
  const [hasToken, setHasToken] = useState(false)

  useEffect(() => {
    if (
      open &&
      ["lark", "wecom_ai_bot", "dingtalk"].includes(channel.channel_type)
    ) {
      getChatChannelHasToken(channel.id)
        .then(setHasToken)
        .catch(() => {})
    }
  }, [open, channel.id, channel.channel_type])

  const handleSubmit = useCallback(async () => {
    if (!name.trim()) {
      setError(t("nameRequired"))
      return
    }
    if (channel.channel_type === "lark" && !chatId.trim()) {
      setError(t("chatIdRequired"))
      return
    }
    if (channel.channel_type === "wecom_ai_bot" && !botId.trim()) {
      setError(t("botIdRequired"))
      return
    }
    if (channel.channel_type === "dingtalk" && !clientId.trim()) {
      setError(t("clientIdRequired"))
      return
    }
    if (
      channel.channel_type === "wecom_ai_bot" &&
      dailyReportEnabled &&
      !chatId.trim()
    ) {
      setError(t("chatIdRequired"))
      return
    }

    setLoading(true)
    setError(null)
    try {
      const configPatchJson = buildChatChannelConfigPatch(
        channel.channel_type,
        {
          appId,
          baseUrl,
          botId,
          chatId,
          clientId,
          defaultAgentType,
        }
      )

      const updated = await updateChatChannel({
        id: channel.id,
        name: name.trim(),
        configPatchJson,
        dailyReportEnabled:
          channel.channel_type !== "dingtalk" && dailyReportEnabled,
        dailyReportTime:
          channel.channel_type !== "dingtalk" && dailyReportEnabled
            ? dailyReportTime
            : null,
      })

      if (token.trim()) {
        await saveChatChannelToken(channel.id, token.trim())
      }
      const status = token.trim()
        ? ((await listChatChannels()).find((item) => item.id === channel.id) ??
          updated)
        : updated

      // IYW-CHANNEL-002: a failed reconnect still saved the edit — surface
      // "已保存，连接失败" instead of closing the dialog silently.
      if (status.runtime_status === "error" && status.last_error) {
        setError(`${t("savedButConnectFailed")}：${status.last_error}`)
        onChannelUpdated()
        return
      }

      onOpenChange(false)
      onChannelUpdated()
      toast.success(t("editSuccess"))
    } catch (err: unknown) {
      const msg = toErrorMessage(err)
      setError(msg)
    } finally {
      setLoading(false)
    }
  }, [
    name,
    token,
    chatId,
    channel,
    appId,
    botId,
    baseUrl,
    clientId,
    defaultAgentType,
    dailyReportEnabled,
    dailyReportTime,
    onOpenChange,
    onChannelUpdated,
    t,
  ])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("editChannel")}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs font-medium">{t("channelName")}</label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("channelNamePlaceholder")}
            />
          </div>

          <ChatChannelCredentialFields
            channelType={channel.channel_type}
            appId={appId}
            botId={botId}
            chatId={chatId}
            clientId={clientId}
            token={token}
            onAppIdChange={setAppId}
            onBotIdChange={setBotId}
            onChatIdChange={setChatId}
            onClientIdChange={setClientId}
            onTokenChange={setToken}
            secretPlaceholder={
              hasToken ? t("tokenPlaceholderKeep") : t("secretRequired")
            }
            showLegacyHint
          />

          {channel.channel_type === "weixin" && baseUrl && (
            <div className="space-y-1.5">
              <label className="text-xs font-medium">Base URL</label>
              <Input value={baseUrl} disabled />
            </div>
          )}

          <div className="space-y-1.5">
            <label className="text-xs font-medium">{t("defaultAgent")}</label>
            <Select
              value={defaultAgentType ?? NO_AGENT}
              onValueChange={(v) =>
                setDefaultAgentType(v === NO_AGENT ? null : (v as AgentType))
              }
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_AGENT}>
                  {t("defaultAgentNone")}
                </SelectItem>
                {installedAgents.map((agent) => (
                  <SelectItem key={agent.agent_type} value={agent.agent_type}>
                    {agent.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              {t("defaultAgentHint")}
            </p>
          </div>

          <ChatChannelDailyReportFields
            channelType={channel.channel_type}
            enabled={dailyReportEnabled}
            time={dailyReportTime}
            onEnabledChange={setDailyReportEnabled}
            onTimeChange={setDailyReportTime}
          />

          {error && (
            <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-400">
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={loading}
          >
            {t("cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={loading}>
            {loading && <Loader2 className="h-3.5 w-3.5 animate-spin mr-1" />}
            {t("save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
