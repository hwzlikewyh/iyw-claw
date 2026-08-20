"use client"

import { ExternalLink, Loader2, ScanLine } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { openUrl } from "@/lib/platform"
import { ChatChannelCredentialFields } from "./chat-channel-credential-fields"

type ParameterChannel = "lark" | "wecom_ai_bot" | "dingtalk"

interface ChannelAuthStepProps {
  channelType: ParameterChannel
  appId: string
  botId: string
  chatId: string
  clientId: string
  token: string
  larkRegion: "feishu" | "lark"
  hasToken: boolean
  loading: boolean
  error: string | null
  onAppIdChange: (value: string) => void
  onBotIdChange: (value: string) => void
  onChatIdChange: (value: string) => void
  onClientIdChange: (value: string) => void
  onTokenChange: (value: string) => void
  onLarkRegionChange: (value: "feishu" | "lark") => void
  onStartQr: () => void
  onSaveCredential: () => void
  onCancel: () => void
}

export function ChannelAuthStep(props: ChannelAuthStepProps) {
  return (
    <div className="grid gap-5 md:grid-cols-[0.9fr_1.1fr]">
      <ChannelGuide channelType={props.channelType} />
      <CredentialSection {...props} />
    </div>
  )
}

function ChannelGuide({ channelType }: { channelType: ParameterChannel }) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <section className="space-y-3 border-b pb-5 md:border-r md:border-b-0 md:pr-5 md:pb-0">
      <h3 className="text-sm font-medium">
        {t(`market.guides.${channelType}.title`)}
      </h3>
      <ol className="space-y-3 text-xs text-muted-foreground">
        {(["one", "two", "three"] as const).map((key, index) => (
          <li key={key} className="flex gap-2.5">
            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-[11px] font-medium text-primary">
              {index + 1}
            </span>
            <span>{t(`market.guides.${channelType}.${key}`)}</span>
          </li>
        ))}
      </ol>
      <Button
        variant="outline"
        size="sm"
        onClick={() => void openUrl(adminUrl(channelType))}
      >
        <ExternalLink className="h-3.5 w-3.5" />
        {t("market.openAdmin")}
      </Button>
    </section>
  )
}

function CredentialSection(props: ChannelAuthStepProps) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <section className="space-y-4">
      {props.channelType === "lark" && <LarkRegionSelector {...props} />}
      <ChatChannelCredentialFields
        channelType={props.channelType}
        appId={props.appId}
        botId={props.botId}
        chatId={props.chatId}
        clientId={props.clientId}
        token={props.token}
        onAppIdChange={props.onAppIdChange}
        onBotIdChange={props.onBotIdChange}
        onChatIdChange={props.onChatIdChange}
        onClientIdChange={props.onClientIdChange}
        onTokenChange={props.onTokenChange}
        secretPlaceholder={
          props.hasToken ? t("tokenPlaceholderKeep") : undefined
        }
      />
      {props.error && <ErrorBox message={props.error} />}
      <QrAction loading={props.loading} onStartQr={props.onStartQr} />
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span className="h-px flex-1 bg-border" />
        {t("qr.manualDivider")}
        <span className="h-px flex-1 bg-border" />
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={props.onCancel}>
          {t("market.keepDraft")}
        </Button>
        <Button onClick={props.onSaveCredential} disabled={props.loading}>
          {props.loading && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
          {t("market.verifyCredential")}
        </Button>
      </div>
    </section>
  )
}

function LarkRegionSelector(props: ChannelAuthStepProps) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium">{t("qr.larkRegion")}</label>
      <div className="grid grid-cols-2 gap-1 rounded-md border p-1">
        {(["feishu", "lark"] as const).map((region) => (
          <Button
            key={region}
            type="button"
            size="sm"
            variant={props.larkRegion === region ? "secondary" : "ghost"}
            onClick={() => props.onLarkRegionChange(region)}
          >
            {t(`qr.regions.${region}`)}
          </Button>
        ))}
      </div>
    </div>
  )
}

function QrAction({
  loading,
  onStartQr,
}: {
  loading: boolean
  onStartQr: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <Button className="w-full" disabled={loading} onClick={onStartQr}>
      {loading ? (
        <Loader2 className="h-3.5 w-3.5 animate-spin" />
      ) : (
        <ScanLine className="h-3.5 w-3.5" />
      )}
      {t("qr.start")}
    </Button>
  )
}

function adminUrl(type: ParameterChannel) {
  if (type === "lark") return "https://open.feishu.cn/app"
  if (type === "dingtalk") return "https://open-dev.dingtalk.com/"
  return "https://work.weixin.qq.com/wework_admin/frame"
}

function ErrorBox({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {message}
    </div>
  )
}
