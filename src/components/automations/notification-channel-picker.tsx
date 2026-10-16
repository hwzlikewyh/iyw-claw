"use client"

import { MessageCircle, RotateCw, Settings2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { openSettingsWindow } from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useAutomationChannels } from "./use-automation-channels"

export function NotificationChannelPicker({
  value,
  onChange,
}: {
  value: number | null
  onChange: (id: number | null) => void
}) {
  const t = useTranslations("Automations")
  const { channels, loading, error, reload } = useAutomationChannels()
  const selected = channels.find((channel) => channel.id === value)
  const status = (channel: (typeof channels)[number]) =>
    !channel.enabled
      ? t("channelDisabled")
      : channel.runtime_status === "connected"
        ? t("channelConnected")
        : t("channelOffline")
  return (
    <section className="space-y-3">
      <h3 className="flex items-center gap-2 text-xs font-medium">
        <MessageCircle className="size-3.5 text-muted-foreground" />
        {t("notificationTitle")}
      </h3>
      <Select
        value={value == null ? "none" : String(value)}
        onValueChange={(id) => onChange(id === "none" ? null : Number(id))}
      >
        <SelectTrigger className="w-full text-xs" disabled={loading}>
          <SelectValue placeholder={t("notificationNone")} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="none">{t("notificationNone")}</SelectItem>
          {value != null && !selected && (
            <SelectItem value={String(value)} disabled>
              {t("channelUnavailable")} #{value}
            </SelectItem>
          )}
          {channels.map((channel) => (
            <SelectItem
              key={channel.id}
              value={String(channel.id)}
              disabled={!channel.enabled && channel.id !== value}
            >
              {channel.name} · {status(channel)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p className="text-xs leading-relaxed text-muted-foreground">
        {t("notificationHint")}
      </p>
      {selected && selected.runtime_status !== "connected" && (
        <p className="text-xs text-amber-600 dark:text-amber-400">
          {status(selected)}
        </p>
      )}
      {error && (
        <p role="alert" className="text-xs text-destructive">
          {t("channelLoadFailed")}: {error}
        </p>
      )}
      <div className="flex flex-wrap items-center justify-between gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void reload()}
        >
          <RotateCw className="size-3" />
          {t("refresh")}
        </Button>
        <Button
          type="button"
          variant="link"
          size="sm"
          onClick={() =>
            void openSettingsWindow("chat-channels").catch((cause) =>
              toast.error(String(cause))
            )
          }
        >
          <Settings2 className="size-3" />
          {t("manageChannels")}
        </Button>
      </div>
    </section>
  )
}
