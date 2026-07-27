"use client"

import { useTranslations } from "next-intl"

import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { ChannelListTab } from "./channel-list-tab"
import { ChannelCommandsTab } from "./channel-commands-tab"
import { ChannelEventsTab } from "./channel-events-tab"
import { ChannelOtherTab } from "./channel-other-tab"
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

      <Tabs defaultValue="channels" className="space-y-4">
        <TabsList>
          <TabsTrigger value="channels">{t("tabs.channels")}</TabsTrigger>
          <TabsTrigger value="commands">{t("tabs.commands")}</TabsTrigger>
          <TabsTrigger value="events">{t("tabs.events")}</TabsTrigger>
          <TabsTrigger value="other">{t("tabs.other")}</TabsTrigger>
        </TabsList>

        <TabsContent value="channels" className="mt-0">
          <ChannelListTab />
        </TabsContent>
        <TabsContent value="commands" className="mt-0">
          <ChannelCommandsTab />
        </TabsContent>
        <TabsContent value="events" className="mt-0">
          <ChannelEventsTab />
        </TabsContent>
        <TabsContent value="other" className="mt-0">
          <ChannelOtherTab />
        </TabsContent>
      </Tabs>
    </SettingsPageLayout>
  )
}
