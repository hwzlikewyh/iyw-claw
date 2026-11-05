"use client"

import { Globe2, PanelsTopLeft } from "lucide-react"
import { useTranslations } from "next-intl"

export function BrowserStartPage() {
  const t = useTranslations("Browser")
  return (
    <div className="flex h-full flex-col items-center justify-center gap-5 bg-background px-6 text-center">
      <div className="relative text-muted-foreground/35" aria-hidden="true">
        <PanelsTopLeft className="size-11 stroke-1" />
        <Globe2 className="absolute -right-1 -bottom-1 size-5 bg-background text-muted-foreground" />
      </div>
      <p className="text-sm text-muted-foreground">{t("newTabHint")}</p>
    </div>
  )
}
