"use client"

import { useEffect, useState } from "react"
import { FileStack, Globe2, MonitorCog } from "lucide-react"
import { useTranslations } from "next-intl"

import { ComputerUseSettings } from "@/components/settings/computer-use-settings"
import { InternetToolsSettings } from "@/components/settings/internet-tools-settings"
import { OfficeToolsSettings } from "@/components/settings/office-tools-settings"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

export type SkillPackCategory =
  | "office-tools"
  | "internet-tools"
  | "computer-use"

interface SkillPacksSettingsProps {
  initialCategory?: SkillPackCategory
}

export function SkillPacksSettings({
  initialCategory = "office-tools",
}: SkillPacksSettingsProps) {
  const t = useTranslations("SkillPacksSettings")
  const [activeCategory, setActiveCategory] =
    useState<SkillPackCategory>(initialCategory)

  useEffect(() => {
    setActiveCategory(initialCategory)
  }, [initialCategory])

  return (
    <Tabs
      value={activeCategory}
      onValueChange={(value) => setActiveCategory(value as SkillPackCategory)}
      className="h-full min-h-0 w-full min-w-0 gap-0 overflow-hidden"
    >
      <div className="min-w-0 shrink-0 overflow-x-auto border-b px-3 py-2 md:px-4">
        <TabsList variant="line" className="w-max min-w-full justify-start">
          <TabsTrigger value="office-tools" className="flex-none">
            <FileStack aria-hidden="true" />
            {t("tabs.officeTools")}
          </TabsTrigger>
          <TabsTrigger value="internet-tools" className="flex-none">
            <Globe2 aria-hidden="true" />
            {t("tabs.internetTools")}
          </TabsTrigger>
          <TabsTrigger value="computer-use" className="flex-none">
            <MonitorCog aria-hidden="true" />
            {t("tabs.computerUse")}
          </TabsTrigger>
        </TabsList>
      </div>

      <div className="min-h-0 w-full min-w-0 flex-1 overflow-hidden">
        {activeCategory === "office-tools" && <OfficeToolsSettings />}
        {activeCategory === "internet-tools" && <InternetToolsSettings />}
        {activeCategory === "computer-use" && <ComputerUseSettings />}
      </div>
    </Tabs>
  )
}
