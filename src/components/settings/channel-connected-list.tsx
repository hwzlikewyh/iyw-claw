"use client"

import { useState } from "react"
import {
  Activity,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  RefreshCw,
  ScanLine,
  Square,
  Trash2,
  Zap,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Switch } from "@/components/ui/switch"
import {
  connectChatChannel,
  deleteChatChannel,
  disconnectChatChannel,
  fullLoopChatChannel,
  quickCheckChatChannel,
  testChatChannel,
  updateChatChannel,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type {
  ChatChannelInfo,
  ChannelReadinessReport,
  ChannelStatusInfo,
  ChannelType,
} from "@/lib/types"
import { DeleteChannelDialog } from "./delete-channel-dialog"

export function ChannelConnectedList({
  channels,
  statuses,
  readiness,
  onReload,
  onEdit,
  onQrAuth,
}: {
  channels: ChatChannelInfo[]
  statuses: ChannelStatusInfo[]
  readiness: ChannelReadinessReport[]
  onReload: () => Promise<void>
  onEdit: (channel: ChatChannelInfo) => void
  onQrAuth: (channel: ChatChannelInfo) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const [loadingId, setLoadingId] = useState<number | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<ChatChannelInfo | null>(null)

  const run = async (
    channel: ChatChannelInfo,
    action: () => Promise<unknown>
  ) => {
    setLoadingId(channel.id)
    try {
      await action()
      await onReload()
    } catch (error) {
      if (channel.channel_type === "weixin") onQrAuth(channel)
      else toast.error(toErrorMessage(error))
    } finally {
      setLoadingId(null)
    }
  }

  const diagnostic = async (channel: ChatChannelInfo, full: boolean) => {
    await run(channel, async () => {
      const result = full
        ? await fullLoopChatChannel(channel.id)
        : await quickCheckChatChannel(channel.id)
      const failed = result.readiness.stages.find((stage) => !stage.ok)
      if (result.roundtrip && !result.roundtrip.verified) {
        throw new Error(result.roundtrip.details.join("; "))
      }
      if (failed) throw new Error(failed.error ?? failed.key)
      toast.success(t("diagnosticOk"))
    })
  }

  if (channels.length === 0) {
    return (
      <div className="border-y py-12 text-center text-sm text-muted-foreground">
        {t("market.noConnected")}
      </div>
    )
  }

  return (
    <>
      <section className="divide-y border-y">
        {channels.map((channel) => {
          const status =
            statuses.find((item) => item.channel_id === channel.id)?.status ??
            "disconnected"
          const report = readiness.find((item) => item.channelId === channel.id)
          const connected = status === "connected"
          const busy = loadingId === channel.id
          return (
            <article
              key={channel.id}
              className="flex flex-col gap-3 py-3 sm:flex-row sm:items-center"
            >
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <h3 className="truncate text-sm font-medium">
                    {channel.name}
                  </h3>
                  <Badge variant="outline" className="text-[10px]">
                    {t(typeLabel(channel.channel_type))}
                  </Badge>
                  <StatusDot status={status} />
                  {channel.channel_type === "wecom_agent" && (
                    <Badge
                      variant={
                        report?.callbackVerified ? "secondary" : "outline"
                      }
                      className="text-[10px]"
                    >
                      {t(
                        report?.callbackVerified
                          ? "market.callbackVerified"
                          : "market.callbackPending"
                      )}
                    </Badge>
                  )}
                  {report?.inboundVerified && (
                    <Badge variant="secondary" className="text-[10px]">
                      {t("market.inboundVerified")}
                    </Badge>
                  )}
                </div>
                <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                  <span>{t(`market.status.${status}`)}</span>
                  {channel.daily_report_enabled && (
                    <span>
                      {t("dailyReport")}: {channel.daily_report_time ?? "18:00"}
                    </span>
                  )}
                  {(report?.lastError || channel.last_error) && (
                    <span className="max-w-full truncate text-destructive">
                      {report?.lastError ?? channel.last_error}
                    </span>
                  )}
                </div>
              </div>
              <div className="flex shrink-0 items-center justify-end gap-1.5">
                <Switch
                  checked={channel.enabled}
                  disabled={busy}
                  aria-label={t("market.toggleChannel")}
                  onCheckedChange={() =>
                    void run(channel, () =>
                      updateChatChannel({
                        id: channel.id,
                        enabled: !channel.enabled,
                      })
                    )
                  }
                />
                <Button
                  variant="outline"
                  size="icon-sm"
                  disabled={busy || !channel.enabled}
                  title={connected ? t("disconnect") : t("connect")}
                  onClick={() =>
                    void run(channel, () =>
                      connected
                        ? disconnectChatChannel(channel.id)
                        : connectChatChannel(channel.id)
                    )
                  }
                >
                  {busy ? (
                    <Loader2 className="animate-spin" />
                  ) : connected ? (
                    <Square />
                  ) : (
                    <Play />
                  )}
                </Button>
                <Button
                  variant="outline"
                  size="icon-sm"
                  disabled={busy || !channel.enabled}
                  title={t("quickCheck")}
                  onClick={() => void diagnostic(channel, false)}
                >
                  <Activity />
                </Button>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      title={t("market.moreActions")}
                    >
                      <MoreHorizontal />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem
                      onSelect={() =>
                        void run(channel, () => testChatChannel(channel.id))
                      }
                    >
                      <Zap />
                      {t("test")}
                    </DropdownMenuItem>
                    {supportsUnifiedQr(channel.channel_type) && (
                      <DropdownMenuItem onSelect={() => onQrAuth(channel)}>
                        <ScanLine />
                        {t("qr.reconnect")}
                      </DropdownMenuItem>
                    )}
                    <DropdownMenuItem
                      disabled={
                        !channel.enabled ||
                        channel.channel_type === "wecom_agent"
                      }
                      onSelect={() => void diagnostic(channel, true)}
                    >
                      <RefreshCw />
                      {channel.channel_type === "wecom_agent"
                        ? t("market.wecomAgent.manualLoopRequired")
                        : t("fullLoop")}
                    </DropdownMenuItem>
                    <DropdownMenuItem
                      disabled={
                        connected && channel.channel_type !== "wecom_agent"
                      }
                      onSelect={() => onEdit(channel)}
                    >
                      <Pencil />
                      {t("editChannel")}
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      variant="destructive"
                      onSelect={() => setDeleteTarget(channel)}
                    >
                      <Trash2 />
                      {t("delete")}
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </article>
          )
        })}
      </section>
      <DeleteChannelDialog
        open={Boolean(deleteTarget)}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
        onConfirm={() =>
          deleteTarget &&
          void run(deleteTarget, async () => {
            await deleteChatChannel(deleteTarget.id)
            setDeleteTarget(null)
          })
        }
      />
    </>
  )
}

function StatusDot({ status }: { status: ChannelStatusInfo["status"] }) {
  const color =
    status === "connected"
      ? "bg-green-500"
      : status === "connecting"
        ? "bg-amber-500 animate-pulse"
        : status === "error"
          ? "bg-destructive"
          : "bg-muted-foreground/40"
  return <span className={`h-2 w-2 rounded-full ${color}`} />
}

function typeLabel(type: ChannelType) {
  if (type === "wecom_ai_bot") return "wecomAiBot"
  if (type === "wecom_agent") return "wecomAgent"
  return type
}

function supportsUnifiedQr(type: ChannelType) {
  return ["weixin", "wecom_ai_bot", "dingtalk", "lark"].includes(type)
}
