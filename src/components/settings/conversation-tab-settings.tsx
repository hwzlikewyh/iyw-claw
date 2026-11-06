"use client"

import { useEffect, useState } from "react"
import { PanelsTopLeft } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Input } from "@/components/ui/input"
import { SettingRow, SettingSection } from "./settings-ui"
import {
  DEFAULT_CONVERSATION_TAB_LIMIT,
  getConversationTabLimit,
  setConversationTabLimit,
  subscribeConversationTabLimit,
} from "@/lib/conversation-tab-preferences"

function saveLimit(value: string, t: ReturnType<typeof useTranslations>) {
  const parsed = value.trim() === "" ? NaN : Number(value)
  if (!Number.isSafeInteger(parsed) || parsed < 0) {
    toast.error(t("invalidLimit"))
    return getConversationTabLimit()
  }
  try {
    setConversationTabLimit(parsed)
    return parsed
  } catch (error) {
    console.error("[conversation-tabs] preference save failed", error)
    toast.error(t("saveFailed"))
    return getConversationTabLimit()
  }
}

export function ConversationTabSettings() {
  const t = useTranslations("ConversationTabs")
  const [value, setValue] = useState(String(DEFAULT_CONVERSATION_TAB_LIMIT))
  useEffect(() => {
    const sync = () => setValue(String(getConversationTabLimit()))
    sync()
    return subscribeConversationTabLimit(sync)
  }, [])
  return (
    <SettingSection icon={PanelsTopLeft} title={t("title")}>
      <SettingRow title={t("limitLabel")} description={t("limitDescription")}>
        <Input
          className="w-24"
          type="number"
          min={0}
          step={1}
          value={value}
          aria-label={t("limitLabel")}
          onChange={(event) => setValue(event.target.value)}
          onBlur={() => setValue(String(saveLimit(value, t)))}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur()
          }}
        />
      </SettingRow>
    </SettingSection>
  )
}
