"use client"

import { ArrowLeft, ExternalLink, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type {
  ChannelConversationInfo,
  ChannelConversationRuntime,
} from "@/lib/channel-runtime"
import { ACTIVE_CHANNEL_PHASES } from "@/lib/channel-runtime"
import { useChannelTranscript } from "@/hooks/use-channel-transcript"
import { ChannelConversationTranscript } from "./channel-conversation-transcript"

export function ChannelConversationDetail({
  channelId,
  item,
  onBack,
}: {
  channelId: number
  item: ChannelConversationInfo
  onBack: () => void
}) {
  const data = useChannelTranscript(channelId, item.conversationId)
  return (
    <article className="flex h-[70vh] min-h-[32rem] min-w-0 flex-col">
      <ConversationHeader
        item={data.data?.runtime.conversation ?? item}
        onBack={onBack}
        onRefresh={data.refresh}
      >
        <ConversationActivity
          runtime={data.data?.runtime ?? null}
          fallback={item}
          error={data.error}
        />
      </ConversationHeader>
      {(data.historyError || data.data?.historyError) && (
        <p role="alert" className="px-4 py-2 text-xs text-destructive">
          {data.historyError ?? data.data?.historyError}
        </p>
      )}
      <ChannelConversationTranscript
        data={data.data}
        older={data.older}
        loading={data.loading}
        before={data.before}
        loadingOlder={data.olderLoading}
        onLoadOlder={data.loadOlder}
      />
      <DeliveryResults runtime={data.data?.runtime ?? null} />
    </article>
  )
}

function ConversationHeader({
  item,
  onBack,
  onRefresh,
  children,
}: {
  item: ChannelConversationInfo
  onBack: () => void
  onRefresh: () => void
  children: React.ReactNode
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const params = new URLSearchParams({
    folderId: String(item.folderId),
    conversationId: String(item.conversationId),
    agent: item.agentType,
  })
  return (
    <div className="space-y-3 border-b p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="min-w-0 truncate text-sm font-semibold">
          {item.title || t("untitled")}
        </h3>
        <div className="flex gap-1">
          <Button
            className="lg:hidden"
            variant="ghost"
            size="sm"
            onClick={onBack}
          >
            <ArrowLeft className="size-3.5" />
            {t("back")}
          </Button>
          <RefreshConversation onRefresh={onRefresh} />
          <Button variant="outline" size="sm" asChild>
            <a href={`/workspace?${params}`}>
              <ExternalLink className="size-3.5" />
              {t("openConversation")}
            </a>
          </Button>
        </div>
      </div>
      {children}
    </div>
  )
}

function RefreshConversation({ onRefresh }: { onRefresh: () => void }) {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={onRefresh}
      title={t("refresh")}
      aria-label={t("refresh")}
    >
      <RefreshCw className="size-3.5" />
    </Button>
  )
}
function ConversationActivity({
  runtime,
  fallback,
  error,
}: {
  runtime: ChannelConversationRuntime | null
  fallback: ChannelConversationInfo
  error: string | null
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const item = runtime?.conversation ?? fallback
  const age = (at: string) =>
    Math.max(
      0,
      Math.floor(
        (Date.parse(runtime?.sampledAt ?? new Date().toISOString()) -
          Date.parse(at)) /
          1_000
      )
    )
  return (
    <div className="space-y-1 text-xs">
      <p role="status" aria-live="polite" className="font-medium">
        {error ? t("phases.unknown") : t(`phases.${item.phase}`)}
      </p>
      <p className="text-muted-foreground">
        {item.agentType} · {item.model || t("unknownModel")}
        {item.startedAt &&
          ACTIVE_CHANNEL_PHASES.has(item.phase) &&
          ` · ${t("elapsed", { seconds: age(item.startedAt) })}`}
      </p>
      {item.lastActivityAt && (
        <p className="text-muted-foreground">
          {t("lastActivity", { seconds: age(item.lastActivityAt) })}
        </p>
      )}
      {item.currentTool && <p className="truncate">{item.currentTool}</p>}
      {(error || item.error) && (
        <p role="alert" className="break-words text-destructive">
          {error ? `${t("refreshFailed")}: ${error}` : item.error}
        </p>
      )}
      {(item.phase === "waiting_confirmation" ||
        item.phase === "waiting_input") && (
        <p className="text-muted-foreground">{t("answerInConversation")}</p>
      )}
    </div>
  )
}

function DeliveryResults({
  runtime,
}: {
  runtime: ChannelConversationRuntime | null
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <details className="max-h-40 overflow-auto border-t px-4 py-2 text-xs">
      <summary className="cursor-pointer">{t("deliveries")}</summary>
      {!runtime?.deliveries.length ? (
        <p className="py-2 text-muted-foreground">{t("noDelivery")}</p>
      ) : (
        <ul className="space-y-2 py-2">
          {runtime.deliveries.map((delivery) => (
            <li key={delivery.id} className="space-y-1">
              <p>
                {t(
                  `delivery.${["sent", "failed", "queued"].includes(delivery.status) ? delivery.status : "unknown"}`
                )}{" "}
                · {new Date(delivery.created_at).toLocaleString()}
              </p>
              <p className="break-words text-muted-foreground">
                {delivery.content_preview}
              </p>
              {delivery.error_detail && (
                <p className="text-destructive">{delivery.error_detail}</p>
              )}
            </li>
          ))}
        </ul>
      )}
    </details>
  )
}
