"use client"

import { useTranslations } from "next-intl"

import { ChannelListTab } from "./channel-list-tab"
import { SendHorizontal } from "lucide-react"
import {
  SettingsPageLayout,
  SettingsPageHeader,
} from "@/components/settings/settings-ui"

export function ChatChannelSettings() {
  const t = useTranslations("ChatChannelSettings")

  return (
    <SettingsPageLayout>
      <SettingsPageHeader
        icon={SendHorizontal}
        title={t("sectionTitle")}
        description={t("sectionDescription")}
      />

      <ChannelListTab />
    </SettingsPageLayout>
  )
}
