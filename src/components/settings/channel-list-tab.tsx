"use client"

import { useCallback, useEffect, useState } from "react"
import {
  Activity,
  AlertCircle,
  Loader2,
  MessageCircle,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Square,
  Trash2,
  Zap,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Switch } from "@/components/ui/switch"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  listChatChannels,
  deleteChatChannel,
  connectChatChannel,
  disconnectChatChannel,
  testChatChannel,
  updateChatChannel,
  getChatChannelStatus,
  getChatChannelReadiness,
  quickCheckChatChannel,
  fullLoopChatChannel,
} from "@/lib/api"
import { subscribe } from "@/lib/platform"
import type {
  ChatChannelInfo,
  ChannelReadinessReport,
  ChannelStatusInfo,
  ChannelType,
} from "@/lib/types"
import { toErrorMessage } from "@/lib/app-error"
import { AddChatChannelDialog } from "./add-chat-channel-dialog"
import { EditChatChannelDialog } from "./edit-chat-channel-dialog"
import { WeixinQrcodeDialog } from "./weixin-qrcode-dialog"

export function ChannelListTab() {
  const t = useTranslations("ChatChannelSettings")
  const [channels, setChannels] = useState<ChatChannelInfo[]>([])
  const [statuses, setStatuses] = useState<ChannelStatusInfo[]>([])
  const [readiness, setReadiness] = useState<ChannelReadinessReport[]>([])
  const [loading, setLoading] = useState(true)
  const [addDialogOpen, setAddDialogOpen] = useState(false)
  const [editTarget, setEditTarget] = useState<ChatChannelInfo | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<ChatChannelInfo | null>(null)
  const [actionLoading, setActionLoading] = useState<number | null>(null)
  const [qrcodeChannelId, setQrcodeChannelId] = useState<number | null>(null)

  const loadChannels = useCallback(async () => {
    try {
      const [chs, sts, rd] = await Promise.all([
        listChatChannels(),
        getChatChannelStatus().catch(() => []),
        getChatChannelReadiness().catch(() => []),
      ])
      setChannels(chs)
      setStatuses(sts)
      setReadiness(rd)
    } catch {
      toast.error(t("loadFailed"))
    } finally {
      setLoading(false)
    }
  }, [t])

  useEffect(() => {
    loadChannels().catch(console.error)
  }, [loadChannels])

  // Subscribe to real-time status change events from backend
  useEffect(() => {
    let cancelled = false
    let unsub: (() => void) | undefined
    subscribe<{
      channel_id: number
      status: ChannelStatusInfo["status"]
    }>("chat-channel://status", (payload) => {
      setStatuses((prev) => {
        const idx = prev.findIndex((s) => s.channel_id === payload.channel_id)
        if (idx >= 0) {
          const updated = [...prev]
          updated[idx] = { ...updated[idx], status: payload.status }
          return updated
        }
        return prev
      })
    }).then((fn) => {
      if (cancelled) fn()
      else unsub = fn
    })
    return () => {
      cancelled = true
      unsub?.()
    }
  }, [])

  const handleToggleEnabled = useCallback(
    async (ch: ChatChannelInfo) => {
      try {
        // IYW-CHANNEL-002: the backend reconcile entry drives both directions
        // (disable → disconnect, enable → connect); the UI only flips the
        // desired state.
        const updated = await updateChatChannel({
          id: ch.id,
          enabled: !ch.enabled,
        })
        if (updated.enabled && updated.runtime_status === "error") {
          toast.error(
            `${t("enabledNotConnected")}${updated.last_error ? "：" + updated.last_error : ""}`
          )
        } else if (!updated.enabled) {
          toast.success(t("disconnectSuccess"))
        }
        await loadChannels()
      } catch {
        toast.error(t("saveFailed"))
      }
    },
    [loadChannels, t]
  )

  const handleConnect = useCallback(
    async (id: number, channelType?: ChannelType) => {
      setActionLoading(id)
      try {
        await connectChatChannel(id)
        toast.success(t("connectSuccess"))
        await loadChannels()
      } catch (err: unknown) {
        if (channelType === "weixin") {
          // No token or token expired — show QR code dialog
          setQrcodeChannelId(id)
        } else {
          const msg = toErrorMessage(err)
          toast.error(t("connectFailed") + ": " + msg)
        }
      } finally {
        setActionLoading(null)
      }
    },
    [loadChannels, t]
  )

  const handleWeixinAuthSuccess = useCallback(
    async (channelId: number) => {
      setQrcodeChannelId(null)
      setActionLoading(channelId)
      try {
        await connectChatChannel(channelId)
        toast.success(t("connectSuccess"))
        await loadChannels()
      } catch (err: unknown) {
        const msg = toErrorMessage(err)
        toast.error(t("connectFailed") + ": " + msg)
      } finally {
        setActionLoading(null)
      }
    },
    [loadChannels, t]
  )

  const handleDisconnect = useCallback(
    async (id: number) => {
      setActionLoading(id)
      try {
        await disconnectChatChannel(id)
        toast.success(t("disconnectSuccess"))
        await loadChannels()
      } catch {
        toast.error(t("disconnectFailed"))
      } finally {
        setActionLoading(null)
      }
    },
    [loadChannels, t]
  )

  const handleTest = useCallback(
    async (id: number) => {
      setActionLoading(id)
      try {
        await testChatChannel(id)
        toast.success(t("testSuccess"))
      } catch (err: unknown) {
        const msg = toErrorMessage(err)
        toast.error(t("testFailed") + ": " + msg)
      } finally {
        setActionLoading(null)
      }
    },
    [t]
  )

  const runDiagnostic = useCallback(
    async (ch: ChatChannelInfo, kind: "quick" | "full") => {
      setActionLoading(ch.id)
      try {
        const result =
          kind === "quick"
            ? await quickCheckChatChannel(ch.id)
            : await fullLoopChatChannel(ch.id)
        const rd = result.readiness
        const failedStage = rd.stages.find((s) => !s.ok)
        if (
          kind === "full" &&
          result.roundtrip &&
          !result.roundtrip.verified
        ) {
          const detail = result.roundtrip.details.join("；")
          toast.error(t("diagnosticFailed") + (detail ? `：${detail}` : ""))
        } else if (failedStage) {
          toast.error(
            `${t("diagnosticFailed")}（${failedStage.key}）：${
              failedStage.error ?? rd.errorMessage ?? ""
            }`
          )
        } else {
          toast.success(t("diagnosticOk"))
        }
        await loadChannels()
      } catch (err: unknown) {
        const msg = toErrorMessage(err)
        toast.error(t("diagnosticFailed") + ": " + msg)
      } finally {
        setActionLoading(null)
      }
    },
    [loadChannels, t]
  )

  const handleDelete = useCallback(async () => {
    if (!deleteTarget) return
    try {
      await deleteChatChannel(deleteTarget.id)
      toast.success(t("deleteSuccess"))
      setDeleteTarget(null)
      await loadChannels()
    } catch {
      toast.error(t("deleteFailed"))
    }
  }, [deleteTarget, loadChannels, t])

  const getChannelStatus = (id: number) =>
    statuses.find((s) => s.channel_id === id)?.status ?? "disconnected"

  const getReadiness = (id: number) =>
    readiness.find((r) => r.channelId === id)

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t("loading")}
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-sm font-medium">{t("channelListTitle")}</h3>
          <p className="text-xs text-muted-foreground">
            {t("channelListDescription")}
          </p>
        </div>
        <Button size="sm" onClick={() => setAddDialogOpen(true)}>
          <Plus className="h-3.5 w-3.5 mr-1" />
          {t("addChannel")}
        </Button>
      </div>

      {channels.length === 0 ? (
        <section className="rounded-xl border bg-card p-8 text-center">
          <MessageCircle className="h-8 w-8 mx-auto text-muted-foreground mb-2" />
          <p className="text-sm text-muted-foreground">{t("noChannels")}</p>
        </section>
      ) : (
        <section className="space-y-2">
          {channels.map((ch) => {
            const status = getChannelStatus(ch.id)
            const isConnected = status === "connected"
            const rd = getReadiness(ch.id)
            const isLoading = actionLoading === ch.id

            return (
              <div
                key={ch.id}
                className="rounded-xl border bg-card p-4 flex items-center gap-4"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-medium">{ch.name}</span>
                    <Badge
                      variant="outline"
                      className="text-xs inline-flex items-center gap-1"
                    >
                      {ch.channel_type}
                      {ch.channel_type === "weixin" && (
                        <TooltipProvider>
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <span
                                role="button"
                                tabIndex={0}
                                className="inline-flex cursor-help rounded-sm text-yellow-600 outline-none focus-visible:ring-1 focus-visible:ring-ring dark:text-yellow-500"
                                aria-label={t("weixinReconnectNotice")}
                              >
                                <AlertCircle className="h-3 w-3" />
                              </span>
                            </TooltipTrigger>
                            <TooltipContent side="top">
                              {t("weixinReconnectNotice")}
                            </TooltipContent>
                          </Tooltip>
                        </TooltipProvider>
                      )}
                    </Badge>
                    <span
                      className={`inline-block h-2 w-2 rounded-full ${
                        isConnected
                          ? "bg-green-500"
                          : status === "connecting"
                            ? "bg-yellow-500 animate-pulse"
                            : status === "error"
                              ? "bg-red-500"
                              : "bg-gray-400"
                      }`}
                    />
                    {ch.enabled && !isConnected && (
                      <Badge variant="destructive" className="text-xs">
                        {t("enabledNotConnected")}
                      </Badge>
                    )}
                  </div>
                  <div className="flex items-center gap-3 mt-1">
                    {ch.daily_report_enabled && (
                      <span className="text-xs text-muted-foreground">
                        {t("dailyReport")}: {ch.daily_report_time || "18:00"}
                      </span>
                    )}
                    {(rd?.lastError || ch.last_error) && (
                      <span
                        className="text-xs text-red-400 truncate max-w-[320px]"
                        title={rd?.lastError ?? ch.last_error ?? undefined}
                      >
                        {t("lastError")}: {rd?.lastError ?? ch.last_error}
                      </span>
                    )}
                  </div>
                </div>

                <div className="flex items-center gap-2">
                  <Switch
                    checked={ch.enabled}
                    onCheckedChange={() => handleToggleEnabled(ch)}
                  />
                  {isConnected ? (
                    <Button
                      variant="destructive"
                      size="sm"
                      title={t("disconnect")}
                      disabled={isLoading}
                      onClick={() => handleDisconnect(ch.id)}
                    >
                      {isLoading ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Square className="h-3.5 w-3.5" />
                      )}
                    </Button>
                  ) : (
                    <Button
                      variant="ghost"
                      size="sm"
                      title={t("connect")}
                      disabled={isLoading || !ch.enabled}
                      onClick={() => handleConnect(ch.id, ch.channel_type)}
                    >
                      {isLoading ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Play className="h-3.5 w-3.5" />
                      )}
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t("test")}
                    disabled={isLoading}
                    onClick={() => handleTest(ch.id)}
                  >
                    <Zap className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t("quickCheck")}
                    disabled={isLoading || !ch.enabled}
                    onClick={() => runDiagnostic(ch, "quick")}
                  >
                    <Activity className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t("fullLoop")}
                    disabled={isLoading || !ch.enabled}
                    onClick={() => runDiagnostic(ch, "full")}
                  >
                    <RefreshCw className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t("editChannel")}
                    disabled={isConnected || isLoading}
                    onClick={() => setEditTarget(ch)}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t("delete")}
                    onClick={() => setDeleteTarget(ch)}
                  >
                    <Trash2 className="h-3.5 w-3.5 text-destructive" />
                  </Button>
                </div>
              </div>
            )
          })}
        </section>
      )}

      <AddChatChannelDialog
        open={addDialogOpen}
        onOpenChange={setAddDialogOpen}
        onChannelAdded={loadChannels}
      />

      {editTarget && (
        <EditChatChannelDialog
          open={!!editTarget}
          channel={editTarget}
          onOpenChange={(open) => !open && setEditTarget(null)}
          onChannelUpdated={loadChannels}
        />
      )}

      {qrcodeChannelId !== null && (
        <WeixinQrcodeDialog
          open
          channelId={qrcodeChannelId}
          onOpenChange={(open) => !open && setQrcodeChannelId(null)}
          onAuthSuccess={handleWeixinAuthSuccess}
        />
      )}

      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("deleteConfirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("deleteConfirmMessage")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("cancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={handleDelete}>
              {t("delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
