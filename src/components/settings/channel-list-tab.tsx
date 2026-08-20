"use client"

import { useCallback, useEffect, useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  createChatChannel,
  deleteChatChannel,
  getChatChannelHasToken,
  getChatChannelReadiness,
  getChatChannelStatus,
  listChatChannels,
  testChatChannel,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { isSetupDraft, parseChannelConfig } from "@/lib/chat-channel-setup"
import { subscribe } from "@/lib/platform"
import type {
  ChatChannelInfo,
  ChannelReadinessReport,
  ChannelStatusInfo,
} from "@/lib/types"
import {
  AddChatChannelDialog,
  type ParameterChannel,
} from "./add-chat-channel-dialog"
import { AbandonChannelDraftDialog } from "./abandon-channel-draft-dialog"
import { ChannelConnectedList } from "./channel-connected-list"
import { ChannelFinalizeDialog } from "./channel-finalize-dialog"
import { ChannelMarket, type MarketType } from "./channel-market"
import { ChatChannelQrcodeDialog } from "./chat-channel-qrcode-dialog"
import { ChannelViewHeader, type ChannelView } from "./channel-view-header"
import { EditChatChannelDialog } from "./edit-chat-channel-dialog"
import { WecomAgentSetupDialog } from "./wecom-agent-setup-dialog"
import { WeixinQrcodeDialog } from "./weixin-qrcode-dialog"
export function ChannelListTab() {
  const t = useTranslations("ChatChannelSettings")
  const [view, setView] = useState<ChannelView>("connected")
  const [channels, setChannels] = useState<ChatChannelInfo[]>([])
  const [statuses, setStatuses] = useState<ChannelStatusInfo[]>([])
  const [readiness, setReadiness] = useState<ChannelReadinessReport[]>([])
  const [loading, setLoading] = useState(true)
  const [parameterSetup, setParameterSetup] = useState<{
    type: ParameterChannel
    draft?: ChatChannelInfo
  } | null>(null)
  const [wecomAgentDraft, setWecomAgentDraft] = useState<
    ChatChannelInfo | undefined
  >()
  const [wecomAgentOpen, setWecomAgentOpen] = useState(false)
  const [weixinDraft, setWeixinDraft] = useState<ChatChannelInfo | null>(null)
  const [qrcodeOpen, setQrcodeOpen] = useState(false)
  const [finalizeChannel, setFinalizeChannel] =
    useState<ChatChannelInfo | null>(null)
  const [editTarget, setEditTarget] = useState<ChatChannelInfo | null>(null)
  const [qrReauthTarget, setQrReauthTarget] = useState<ChatChannelInfo | null>(
    null
  )
  const [abandonTarget, setAbandonTarget] = useState<ChatChannelInfo | null>(
    null
  )
  const loadChannels = useCallback(async () => {
    try {
      const [items, live, reports] = await Promise.all([
        listChatChannels(),
        getChatChannelStatus().catch(() => []),
        getChatChannelReadiness().catch(() => []),
      ])
      setChannels(items)
      setStatuses(live)
      setReadiness(reports)
    } catch {
      toast.error(t("loadFailed"))
    } finally {
      setLoading(false)
    }
  }, [t])
  useEffect(() => {
    void loadChannels()
  }, [loadChannels])

  useEffect(() => {
    let unsubscribe: (() => void) | undefined
    let cancelled = false
    subscribe<{ channel_id: number; status: ChannelStatusInfo["status"] }>(
      "chat-channel://status",
      (payload) => {
        setStatuses((current) => {
          const rest = current.filter(
            (item) => item.channel_id !== payload.channel_id
          )
          const channel = channels.find(
            (item) => item.id === payload.channel_id
          )
          return channel
            ? [
                ...rest,
                {
                  channel_id: channel.id,
                  name: channel.name,
                  channel_type: channel.channel_type,
                  status: payload.status,
                },
              ]
            : current
        })
      }
    ).then((dispose) => {
      if (cancelled) dispose()
      else unsubscribe = dispose
    })
    return () => {
      cancelled = true
      unsubscribe?.()
    }
  }, [channels])

  const drafts = useMemo(() => channels.filter(isSetupDraft), [channels])
  const connected = useMemo(
    () => channels.filter((channel) => !isSetupDraft(channel)),
    [channels]
  )
  const startWeixin = async (draft?: ChatChannelInfo) => {
    try {
      const channel =
        draft ??
        (await createChatChannel({
          name: t("market.draftNames.weixin"),
          channelType: "weixin",
          configJson: JSON.stringify({
            base_url: "https://ilinkai.weixin.qq.com",
            setup_state: "pending_auth",
          }),
          enabled: false,
          dailyReportEnabled: false,
        }))
      if (await getChatChannelHasToken(channel.id)) {
        try {
          await testChatChannel(channel.id)
          setFinalizeChannel(channel)
          await loadChannels()
          return
        } catch {
          toast.error(t("market.weixinReauthRequired"))
        }
      }
      setWeixinDraft(channel)
      setQrcodeOpen(true)
      await loadChannels()
    } catch {
      toast.error(t("saveFailed"))
    }
  }
  const openSetup = (type: MarketType, draft?: ChatChannelInfo) => {
    if (type === "weixin") {
      void startWeixin(draft)
      return
    }
    if (type === "wecom_agent") {
      setWecomAgentDraft(draft)
      setWecomAgentOpen(true)
      return
    }
    setParameterSetup({ type, draft })
  }

  const completeSetup = async () => {
    setParameterSetup(null)
    setWecomAgentOpen(false)
    setWecomAgentDraft(undefined)
    setQrcodeOpen(false)
    setWeixinDraft(null)
    setFinalizeChannel(null)
    await loadChannels()
    setView("connected")
  }
  const closeParameterSetup = (open: boolean) => {
    if (open) return
    setParameterSetup(null)
    void loadChannels()
  }

  const closeWecomAgentSetup = (open: boolean) => {
    setWecomAgentOpen(open)
    if (open) return
    setWecomAgentDraft(undefined)
    void loadChannels()
  }
  const handleEdit = (channel: ChatChannelInfo) => {
    if (channel.channel_type === "wecom_agent") {
      openSetup("wecom_agent", channel)
      return
    }
    setEditTarget(channel)
  }

  const abandonDraft = async () => {
    if (!abandonTarget) return
    try {
      await deleteChatChannel(abandonTarget.id)
      setAbandonTarget(null)
      await loadChannels()
    } catch (error) {
      toast.error(toErrorMessage(error))
    }
  }
  if (loading) {
    return (
      <div className="flex min-h-40 items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <ChannelViewHeader
        view={view}
        connectedCount={connected.length}
        draftCount={drafts.length}
        onViewChange={setView}
      />

      {view === "connected" ? (
        <ChannelConnectedList
          channels={connected}
          statuses={statuses}
          readiness={readiness}
          onReload={loadChannels}
          onEdit={handleEdit}
          onQrAuth={setQrReauthTarget}
        />
      ) : (
        <ChannelMarket
          drafts={drafts}
          onStart={(type) => openSetup(type)}
          onContinue={(channel) =>
            openSetup(channel.channel_type as MarketType, channel)
          }
          onAbandon={setAbandonTarget}
        />
      )}

      {parameterSetup && (
        <AddChatChannelDialog
          open
          channelType={parameterSetup.type}
          draft={parameterSetup.draft}
          onOpenChange={closeParameterSetup}
          onChannelAdded={() => void completeSetup()}
        />
      )}
      {wecomAgentOpen && (
        <WecomAgentSetupDialog
          open
          draft={wecomAgentDraft}
          onOpenChange={closeWecomAgentSetup}
          onComplete={() => void completeSetup()}
        />
      )}
      {qrcodeOpen && weixinDraft && (
        <WeixinQrcodeDialog
          open
          channelId={weixinDraft.id}
          onOpenChange={(open) => {
            setQrcodeOpen(open)
            if (!open) setWeixinDraft(null)
          }}
          onAuthSuccess={() => {
            setFinalizeChannel(weixinDraft)
            setQrcodeOpen(false)
            setWeixinDraft(null)
          }}
        />
      )}
      {qrReauthTarget && isUnifiedQrChannel(qrReauthTarget.channel_type) && (
        <ChatChannelQrcodeDialog
          open
          channelId={qrReauthTarget.id}
          channelType={qrReauthTarget.channel_type}
          variant={larkRegion(qrReauthTarget)}
          onOpenChange={(open) => !open && setQrReauthTarget(null)}
          onAuthSuccess={() => {
            setQrReauthTarget(null)
            void loadChannels()
          }}
        />
      )}
      {finalizeChannel && (
        <ChannelFinalizeDialog
          open
          channel={finalizeChannel}
          onOpenChange={(open) => !open && setFinalizeChannel(null)}
          onComplete={() => void completeSetup()}
        />
      )}
      {editTarget && (
        <EditChatChannelDialog
          open
          channel={editTarget}
          onOpenChange={(open) => !open && setEditTarget(null)}
          onChannelUpdated={() => void loadChannels()}
        />
      )}
      <AbandonChannelDraftDialog
        open={Boolean(abandonTarget)}
        onOpenChange={(open) => !open && setAbandonTarget(null)}
        onConfirm={() => void abandonDraft()}
      />
    </div>
  )
}

type UnifiedQrChannel = "weixin" | "wecom_ai_bot" | "dingtalk" | "lark"

function isUnifiedQrChannel(type: string): type is UnifiedQrChannel {
  return ["weixin", "wecom_ai_bot", "dingtalk", "lark"].includes(type)
}

function larkRegion(channel: ChatChannelInfo) {
  if (channel.channel_type !== "lark") return undefined
  return parseChannelConfig(channel).lark_region === "lark" ? "lark" : "feishu"
}
