"use client"

import { useTranslations } from "next-intl"

import { Input } from "@/components/ui/input"
import { WecomAuthPanel } from "@/components/settings/wecom-auth-panel"
import type { ChannelType } from "@/lib/types"

interface ChatChannelCredentialFieldsProps {
  channelType: ChannelType
  appId: string
  botId: string
  chatId: string
  clientId: string
  token: string
  onAppIdChange: (value: string) => void
  onBotIdChange: (value: string) => void
  onChatIdChange: (value: string) => void
  onClientIdChange: (value: string) => void
  onTokenChange: (value: string) => void
  secretPlaceholder?: string
  showLegacyHint?: boolean
}

export function ChatChannelCredentialFields({
  channelType,
  appId,
  botId,
  chatId,
  clientId,
  token,
  onAppIdChange,
  onBotIdChange,
  onChatIdChange,
  onClientIdChange,
  onTokenChange,
  secretPlaceholder,
  showLegacyHint = false,
}: ChatChannelCredentialFieldsProps) {
  const t = useTranslations("ChatChannelSettings")
  const secretInput = (label: string) => (
    <div className="space-y-1.5">
      <label className="text-xs font-medium">{label}</label>
      <Input
        type="password"
        value={token}
        onChange={(event) => onTokenChange(event.target.value)}
        placeholder={secretPlaceholder}
      />
    </div>
  )

  if (channelType === "wecom") {
    return (
      <>
        <WecomAuthPanel />
        {showLegacyHint && (
          <p className="text-xs text-muted-foreground">
            {t("wecomLegacyMigrationHint")}
          </p>
        )}
      </>
    )
  }
  if (channelType === "wecom_ai_bot") {
    return (
      <>
        <div className="space-y-1.5">
          <label className="text-xs font-medium">{t("botId")}</label>
          <Input
            value={botId}
            onChange={(event) => onBotIdChange(event.target.value)}
          />
        </div>
        {secretInput(t("botSecret"))}
        <div className="space-y-1.5">
          <label className="text-xs font-medium">{t("defaultChatId")}</label>
          <Input
            value={chatId}
            onChange={(event) => onChatIdChange(event.target.value)}
          />
        </div>
      </>
    )
  }
  if (channelType === "lark") {
    return (
      <>
        <div className="space-y-1.5">
          <label className="text-xs font-medium">App ID</label>
          <Input
            value={appId}
            onChange={(event) => onAppIdChange(event.target.value)}
            placeholder="cli_xxxxx"
          />
        </div>
        {secretInput("App Secret")}
        <div className="space-y-1.5">
          <label className="text-xs font-medium">Chat ID</label>
          <Input
            value={chatId}
            onChange={(event) => onChatIdChange(event.target.value)}
            placeholder="oc_xxxxx"
          />
        </div>
      </>
    )
  }
  if (channelType === "dingtalk") {
    return (
      <>
        <div className="space-y-1.5">
          <label className="text-xs font-medium">{t("clientId")}</label>
          <Input
            value={clientId}
            onChange={(event) => onClientIdChange(event.target.value)}
          />
        </div>
        {secretInput(t("clientSecret"))}
      </>
    )
  }
  return null
}
