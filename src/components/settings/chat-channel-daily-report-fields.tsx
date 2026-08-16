"use client"

import { useTranslations } from "next-intl"

import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import type { ChannelType } from "@/lib/types"

interface ChatChannelDailyReportFieldsProps {
  channelType: ChannelType
  enabled: boolean
  time: string
  onEnabledChange: (enabled: boolean) => void
  onTimeChange: (time: string) => void
}

export function ChatChannelDailyReportFields({
  channelType,
  enabled,
  time,
  onEnabledChange,
  onTimeChange,
}: ChatChannelDailyReportFieldsProps) {
  const t = useTranslations("ChatChannelSettings")
  if (channelType === "dingtalk") {
    return null
  }
  return (
    <>
      <div className="flex items-center justify-between">
        <label className="text-xs font-medium">{t("dailyReport")}</label>
        <Switch checked={enabled} onCheckedChange={onEnabledChange} />
      </div>
      {enabled && (
        <div className="space-y-1.5">
          <label className="text-xs font-medium">{t("dailyReportTime")}</label>
          <Input
            type="time"
            value={time}
            onChange={(event) => onTimeChange(event.target.value)}
          />
        </div>
      )}
    </>
  )
}
