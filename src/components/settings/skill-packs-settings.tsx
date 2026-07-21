"use client"

import { useEffect, useState } from "react"
import { FileStack, Globe2, Sparkles, TerminalSquare } from "lucide-react"
import { useTranslations } from "next-intl"

import { CodexNativeSettings } from "@/components/settings/codex-native-settings"
import { ExpertsSettings } from "@/components/settings/experts-settings"
import { InternetToolsSettings } from "@/components/settings/internet-tools-settings"
import { OfficeToolsSettings } from "@/components/settings/office-tools-settings"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"

export type SkillPackCategory =
  | "experts"
  | "office-tools"
  | "internet-tools"
  | "codex-native"

interface SkillPacksSettingsProps {
  initialCategory?: SkillPackCategory
}

export function SkillPacksSettings({
  initialCategory = "experts",
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
          <TabsTrigger value="experts" className="flex-none">
            <Sparkles aria-hidden="true" />
            {t("tabs.experts")}
          </TabsTrigger>
          <TabsTrigger value="office-tools" className="flex-none">
            <FileStack aria-hidden="true" />
            {t("tabs.officeTools")}
          </TabsTrigger>
          <TabsTrigger value="internet-tools" className="flex-none">
            <Globe2 aria-hidden="true" />
            {t("tabs.internetTools")}
          </TabsTrigger>
          <TabsTrigger value="codex-native" className="flex-none">
            <TerminalSquare aria-hidden="true" />
            {t("tabs.codexNative")}
          </TabsTrigger>
        </TabsList>
      </div>

      <div className="min-h-0 w-full min-w-0 flex-1 overflow-hidden">
        {activeCategory === "experts" && <ExpertsSettings />}
        {activeCategory === "office-tools" && <OfficeToolsSettings />}
        {activeCategory === "internet-tools" && <InternetToolsSettings />}
        {activeCategory === "codex-native" && <CodexNativeSettings />}
      </div>
    </Tabs>
  )
}
