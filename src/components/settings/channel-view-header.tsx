"use client"

import { Plus } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"

export type ChannelView = "connected" | "market"

export function ChannelViewHeader({
  view,
  connectedCount,
  draftCount,
  onViewChange,
}: {
  view: ChannelView
  connectedCount: number
  draftCount: number
  onViewChange: (view: ChannelView) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const views: ChannelView[] = ["connected", "market"]
  return (
    <div className="space-y-3">
      <div>
        <h3 className="text-sm font-medium">{t("market.title")}</h3>
        <p className="text-xs text-muted-foreground">
          {t("market.description")}
        </p>
      </div>
      <div className="flex w-full flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="grid w-full grid-cols-2 rounded-md border bg-muted/40 p-1 sm:w-auto">
          {views.map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => onViewChange(item)}
              className={`min-w-0 whitespace-nowrap rounded-sm px-3 py-1.5 text-xs transition-colors sm:min-w-28 ${
                view === item
                  ? "bg-background font-medium shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t(`market.views.${item}`, {
                count: item === "connected" ? connectedCount : draftCount,
              })}
            </button>
          ))}
        </div>
        {view === "connected" && (
          <Button
            size="sm"
            className="w-full sm:w-auto"
            onClick={() => onViewChange("market")}
          >
            <Plus className="h-3.5 w-3.5" />
            {t("addChannel")}
          </Button>
        )}
      </div>
    </div>
  )
}
