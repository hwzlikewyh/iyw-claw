import { useTranslations } from "next-intl"

import type { InternetChannelStatus } from "@/lib/types"

function channelTone(status: InternetChannelStatus["status"]): string {
  return {
    ok: "bg-green-500",
    warn: "bg-amber-500",
    error: "bg-destructive",
    off: "bg-muted-foreground",
  }[status]
}

export function InternetChannelPanel({
  channels,
}: {
  channels: InternetChannelStatus[]
}) {
  const t = useTranslations("InternetToolsSettings")
  return (
    <section className="mt-4">
      <div className="mb-2">
        <h3 className="text-sm font-semibold">{t("channelsTitle")}</h3>
        <p className="text-xs text-muted-foreground">
          {t("channelsDescription")}
        </p>
      </div>
      <div className="max-h-52 overflow-y-auto border bg-card">
        {channels.length === 0 ? (
          <p className="p-4 text-sm text-muted-foreground">
            {t("channelsEmpty")}
          </p>
        ) : (
          channels.map((channel) => (
            <div
              key={channel.id}
              className="flex gap-3 border-b px-3 py-2.5 last:border-b-0"
            >
              <span
                className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${channelTone(channel.status)}`}
              />
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium">{channel.name}</span>
                  {channel.activeBackend ? (
                    <span className="text-xs text-muted-foreground">
                      {t("activeBackend", { backend: channel.activeBackend })}
                    </span>
                  ) : null}
                </div>
                <p className="whitespace-pre-wrap break-words text-xs text-muted-foreground">
                  {channel.message}
                </p>
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  )
}
