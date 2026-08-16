"use client"

import { useCallback, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

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
  createChatChannel,
  listChatChannels,
  saveChatChannelToken,
} from "@/lib/api"
import { buildChatChannelConfig } from "@/lib/chat-channel-config"
import type { AgentType, ChannelType } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"

interface AddChatChannelDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onChannelAdded: () => void
}

const NO_AGENT = "__none__"

export function AddChatChannelDialog({
  open,
  onOpenChange,
  onChannelAdded,
}: AddChatChannelDialogProps) {
  const t = useTranslations("ChatChannelSettings")
  const { agents } = useAcpAgents()
  const installedAgents = agents.filter(
    (agent) => agent.enabled && agent.installed_version
  )
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [name, setName] = useState("")
  const [channelType, setChannelType] = useState<ChannelType>("wecom_ai_bot")
  const [token, setToken] = useState("")
  const [chatId, setChatId] = useState("")
  const [appId, setAppId] = useState("")
  const [botId, setBotId] = useState("")
  const [clientId, setClientId] = useState("")
  const [baseUrl, setBaseUrl] = useState("https://ilinkai.weixin.qq.com")
  const [defaultAgentType, setDefaultAgentType] = useState<AgentType | null>(
    null
  )
  const [dailyReportEnabled, setDailyReportEnabled] = useState(false)
  const [dailyReportTime, setDailyReportTime] = useState("18:00")

  const resetForm = useCallback(() => {
    setName("")
    setChannelType("wecom_ai_bot")
    setToken("")
    setChatId("")
    setAppId("")
    setBotId("")
    setClientId("")
    setBaseUrl("https://ilinkai.weixin.qq.com")
    setDefaultAgentType(null)
    setDailyReportEnabled(false)
    setDailyReportTime("18:00")
    setError(null)
  }, [])

  const handleOpenChange = useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) resetForm()
      onOpenChange(nextOpen)
    },
    [onOpenChange, resetForm]
  )

  const handleSubmit = useCallback(async () => {
    if (!name.trim()) {
      setError(t("nameRequired"))
      return
    }
    if (
      ["lark", "wecom_ai_bot", "dingtalk"].includes(channelType) &&
      !token.trim()
    ) {
      setError(t("secretRequired"))
      return
    }
    if (channelType === "lark" && !chatId.trim()) {
      setError(t("chatIdRequired"))
      return
    }
    if (channelType === "wecom_ai_bot" && !botId.trim()) {
      setError(t("botIdRequired"))
      return
    }
    if (channelType === "dingtalk" && !clientId.trim()) {
      setError(t("clientIdRequired"))
      return
    }
    if (
      channelType === "wecom_ai_bot" &&
      dailyReportEnabled &&
      !chatId.trim()
    ) {
      setError(t("chatIdRequired"))
      return
    }

    setLoading(true)
    setError(null)
    try {
      const configJson = buildChatChannelConfig(channelType, {
        appId,
        baseUrl,
        botId,
        chatId,
        clientId,
        defaultAgentType,
      })

      const channel = await createChatChannel({
        name: name.trim(),
        channelType,
        configJson,
        enabled: true,
        dailyReportEnabled: channelType !== "dingtalk" && dailyReportEnabled,
        dailyReportTime:
          channelType !== "dingtalk" && dailyReportEnabled
            ? dailyReportTime
            : null,
      })

      if (
        ["lark", "wecom_ai_bot", "dingtalk"].includes(channelType) &&
        token.trim()
      ) {
        await saveChatChannelToken(channel.id, token.trim())
      }

      // IYW-CHANNEL-001/002: an enabled channel is reconciled (connected)
      // by the backend on create. The create-time reconcile runs before the
      // lark token is saved, so re-read the row to get the final runtime
      // state; a failed connect still saved the channel and must be surfaced
      // ("已保存，连接失败") instead of closing the dialog silently.
      const refreshed = (await listChatChannels()).find(
        (c) => c.id === channel.id
      )
      const status = refreshed ?? channel
      if (status.runtime_status === "error" && status.last_error) {
        setError(`${t("savedButConnectFailed")}：${status.last_error}`)
        onChannelAdded()
        return
      }

      handleOpenChange(false)
      onChannelAdded()
    } catch (err) {
      const msg = toErrorMessage(err)
      setError(msg)
    } finally {
      setLoading(false)
    }
  }, [
    name,
    token,
    chatId,
    channelType,
    appId,
    botId,
    baseUrl,
    defaultAgentType,
    clientId,
    dailyReportEnabled,
    dailyReportTime,
    handleOpenChange,
    onChannelAdded,
    t,
  ])

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("addChannel")}</DialogTitle>
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

          <div className="space-y-1.5">
            <label className="text-xs font-medium">{t("channelType")}</label>
            <Select
              value={channelType}
              onValueChange={(value) => {
                setChannelType(value as ChannelType)
                setToken("")
                setChatId("")
                setAppId("")
                setBotId("")
                setClientId("")
                setDailyReportEnabled(false)
                setError(null)
              }}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="wecom_ai_bot">{t("wecomAiBot")}</SelectItem>
                <SelectItem value="lark">{t("lark")}</SelectItem>
                <SelectItem value="weixin">{t("weixin")}</SelectItem>
                <SelectItem value="dingtalk">{t("dingtalk")}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <ChatChannelCredentialFields
            channelType={channelType}
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
          />

          {channelType === "weixin" && (
            <p className="text-xs text-muted-foreground">
              {t("weixinScanDescription")}
            </p>
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
            channelType={channelType}
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
            onClick={() => handleOpenChange(false)}
            disabled={loading}
          >
            {t("cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={loading}>
            {loading && <Loader2 className="h-3.5 w-3.5 animate-spin mr-1" />}
            {t("create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
