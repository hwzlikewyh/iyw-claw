"use client"

import { useEffect, useMemo, useState } from "react"
import { ExternalLink, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ChatChannelCredentialFields } from "./chat-channel-credential-fields"
import {
  ChannelFinalizeForm,
  type ChannelFinalizeValues,
} from "./channel-finalize-form"
import {
  createChatChannel,
  getChatChannelHasToken,
  saveChatChannelToken,
  testChatChannel,
  updateChatChannel,
} from "@/lib/api"
import { openUrl } from "@/lib/platform"
import { parseChannelConfig } from "@/lib/chat-channel-setup"
import type { ChatChannelInfo } from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"

type ParameterChannel = "lark" | "wecom_ai_bot" | "dingtalk"

export function AddChatChannelDialog({
  open,
  channelType,
  draft,
  onOpenChange,
  onChannelAdded,
}: {
  open: boolean
  channelType: ParameterChannel
  draft?: ChatChannelInfo
  onOpenChange: (open: boolean) => void
  onChannelAdded: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const stored = useMemo(
    () => (draft ? parseChannelConfig(draft) : {}),
    [draft]
  )
  const [step, setStep] = useState<1 | 2>(1)
  const [working, setWorking] = useState<ChatChannelInfo | undefined>(draft)
  const [token, setToken] = useState("")
  const [appId, setAppId] = useState(stored.app_id ?? "")
  const [botId, setBotId] = useState(stored.bot_id ?? "")
  const [clientId, setClientId] = useState(stored.client_id ?? "")
  const [chatId, setChatId] = useState(
    stored.chat_id ?? stored.default_chatid ?? ""
  )
  const [hasToken, setHasToken] = useState(false)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open || !draft) return
    getChatChannelHasToken(draft.id)
      .then(setHasToken)
      .catch(() => {})
  }, [draft, open])

  const validate = () => {
    if (!token.trim() && !hasToken) return t("secretRequired")
    if (channelType === "lark" && (!appId.trim() || !chatId.trim())) {
      return t("market.larkFieldsRequired")
    }
    if (channelType === "wecom_ai_bot" && !botId.trim()) {
      return t("botIdRequired")
    }
    if (channelType === "dingtalk" && !clientId.trim()) {
      return t("clientIdRequired")
    }
    return null
  }

  const config = () => {
    const base = { setup_state: "pending_auth" }
    if (channelType === "lark") {
      return JSON.stringify({
        ...base,
        app_id: appId.trim(),
        chat_id: chatId.trim(),
      })
    }
    if (channelType === "wecom_ai_bot") {
      return JSON.stringify({
        ...base,
        bot_id: botId.trim(),
        default_chatid: chatId.trim(),
      })
    }
    return JSON.stringify({ ...base, client_id: clientId.trim() })
  }

  const saveCredential = async () => {
    const validation = validate()
    if (validation) {
      setError(validation)
      return
    }
    setLoading(true)
    setError(null)
    try {
      let channel = working
      if (!channel) {
        channel = await createChatChannel({
          name: t(`market.draftNames.${channelType}`),
          channelType,
          configJson: config(),
          enabled: false,
          dailyReportEnabled: false,
        })
      } else {
        const patch =
          channelType === "lark"
            ? { appId: appId.trim(), chatId: chatId.trim() }
            : channelType === "wecom_ai_bot"
              ? { botId: botId.trim(), defaultChatid: chatId.trim() }
              : { clientId: clientId.trim() }
        channel = await updateChatChannel({
          id: channel.id,
          configPatchJson: JSON.stringify({
            ...patch,
            setupState: "pending_auth",
          }),
        })
      }
      if (token.trim()) await saveChatChannelToken(channel.id, token.trim())
      setWorking(channel)
      await testChatChannel(channel.id)
      setStep(2)
    } catch (caught) {
      setError(toErrorMessage(caught))
    } finally {
      setLoading(false)
    }
  }

  const finalize = async (values: ChannelFinalizeValues) => {
    if (!working) return
    setLoading(true)
    setError(null)
    try {
      const updated = await updateChatChannel({
        id: working.id,
        name: values.name,
        enabled: true,
        configPatchJson: JSON.stringify({
          setupState: "ready",
          defaultAgentType: values.defaultAgentType,
        }),
        dailyReportEnabled:
          channelType !== "dingtalk" && values.dailyReportEnabled,
        dailyReportTime:
          channelType !== "dingtalk" && values.dailyReportEnabled
            ? values.dailyReportTime
            : null,
      })
      if (updated.runtime_status === "error") {
        setError(updated.last_error ?? t("savedButConnectFailed"))
        return
      }
      onOpenChange(false)
      onChannelAdded()
    } catch (caught) {
      setError(toErrorMessage(caught))
    } finally {
      setLoading(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {step === 1
              ? t("market.configureTitle", {
                  channel: t(channelTypeLabel(channelType)),
                })
              : t("market.finishSetup")}
          </DialogTitle>
        </DialogHeader>
        {step === 1 ? (
          <div className="grid gap-5 md:grid-cols-[0.9fr_1.1fr]">
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
            <section className="space-y-4">
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
                secretPlaceholder={
                  hasToken ? t("tokenPlaceholderKeep") : undefined
                }
              />
              {error && <ErrorBox message={error} />}
              <div className="flex justify-end gap-2">
                <Button variant="outline" onClick={() => onOpenChange(false)}>
                  {t("market.keepDraft")}
                </Button>
                <Button
                  onClick={() => void saveCredential()}
                  disabled={loading}
                >
                  {loading && (
                    <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />
                  )}
                  {t("market.verifyCredential")}
                </Button>
              </div>
            </section>
          </div>
        ) : (
          <ChannelFinalizeForm
            channelType={channelType}
            initialName={working?.name ?? t(`market.draftNames.${channelType}`)}
            submitting={loading}
            error={error}
            onCancel={() => onOpenChange(false)}
            onSubmit={(values) => void finalize(values)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

function channelTypeLabel(type: ParameterChannel) {
  return type === "wecom_ai_bot" ? "wecomAiBot" : type
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

export type { ParameterChannel }
