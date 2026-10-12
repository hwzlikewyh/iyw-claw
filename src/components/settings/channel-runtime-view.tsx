"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import type { ChatChannelInfo, ChannelStatusInfo } from "@/lib/types"
import { onTransportChange } from "@/lib/transport"
import { ChannelConversationBrowser } from "./channel-conversation-browser"
import { ChannelRuntimeProviders } from "./channel-runtime-providers"

interface Props {
  channels: ChatChannelInfo[]
  statuses: ChannelStatusInfo[]
  initialChannelId: number | null
}

export function ChannelRuntimeView(props: Props) {
  return (
    <ChannelRuntimeProviders>
      <RuntimeViewBody {...props} />
    </ChannelRuntimeProviders>
  )
}

function RuntimeViewBody({ channels, statuses, initialChannelId }: Props) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const [channelId, setChannelId] = useState(
    initialChannelId ?? channels[0]?.id ?? null
  )
  const [backend, setBackend] = useState(0)
  useEffect(() => onTransportChange(() => setBackend((value) => value + 1)), [])
  const channel = channels.find((item) => item.id === channelId) ?? channels[0]
  const status = statuses.find(
    (item) => item.channel_id === channel?.id
  )?.status
  if (!channels.length) return <NoChannels />
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm">
          {t("channel")}
          <select
            value={channel?.id ?? ""}
            onChange={(event) => setChannelId(Number(event.target.value))}
            className="h-9 max-w-64 rounded-md border bg-background px-3 text-sm"
          >
            {channels.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <ChannelTransport status={status} />
      </div>
      {channel && (
        <ChannelConversationBrowser
          key={`${backend}:${channel.id}`}
          channel={channel}
        />
      )}
    </section>
  )
}

function NoChannels() {
  const t = useTranslations("ChatChannelSettings.runtime")
  return (
    <p className="border-y py-12 text-center text-sm text-muted-foreground">
      {t("noChannels")}
    </p>
  )
}

function ChannelTransport({
  status,
}: {
  status?: ChannelStatusInfo["status"]
}) {
  const t = useTranslations("ChatChannelSettings.runtime")
  const channelT = useTranslations("ChatChannelSettings")
  return (
    <span className="text-xs text-muted-foreground">
      {t("transport")}:{" "}
      {status ? channelT(`market.status.${status}`) : t("phases.unknown")}
    </span>
  )
}
