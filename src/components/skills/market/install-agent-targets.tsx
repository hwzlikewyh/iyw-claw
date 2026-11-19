"use client"

import { Bot, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"

export function AgentTargets({
  count,
  fresh,
}: {
  count: number
  fresh: boolean
}) {
  const t = useTranslations("SkillMarketV2")

  if (!fresh) {
    return (
      <div className="flex h-20 items-center justify-center text-xs text-muted-foreground">
        <Loader2 className="mr-2 size-3.5 animate-spin" aria-hidden="true" />
        {t("install.targetsLoading")}
      </div>
    )
  }
  if (!count) {
    return (
      <div className="flex min-h-20 items-center gap-3 border-y py-3 text-xs text-muted-foreground">
        <Bot className="size-4 shrink-0" aria-hidden="true" />
        {t("install.noTargets")}
      </div>
    )
  }
  return (
    <p className="border-y py-3 text-xs text-muted-foreground">
      {t("install.profileRule")}
    </p>
  )
}
