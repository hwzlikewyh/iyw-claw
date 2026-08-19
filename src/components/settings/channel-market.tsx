"use client"

import { useMemo, useState, type ComponentType } from "react"
import {
  Bird,
  Bot,
  Building2,
  MessageCircle,
  RadioTower,
  Search,
  Trash2,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { parseChannelConfig } from "@/lib/chat-channel-setup"
import type { ChatChannelInfo, ChannelType } from "@/lib/types"

type MarketType = Exclude<ChannelType, "wecom">
type AccessMode = "all" | "scan" | "stream" | "callback"

interface MarketCard {
  type: MarketType
  icon: ComponentType<{ className?: string }>
  mode: Exclude<AccessMode, "all">
  tag: "recommended" | "noPublic" | "httpsRequired"
}

const CARDS: MarketCard[] = [
  { type: "weixin", icon: MessageCircle, mode: "scan", tag: "recommended" },
  { type: "wecom_ai_bot", icon: Bot, mode: "stream", tag: "noPublic" },
  {
    type: "wecom_agent",
    icon: Building2,
    mode: "callback",
    tag: "httpsRequired",
  },
  { type: "lark", icon: Bird, mode: "stream", tag: "noPublic" },
  { type: "dingtalk", icon: RadioTower, mode: "stream", tag: "noPublic" },
]

export function ChannelMarket({
  drafts,
  onStart,
  onContinue,
  onAbandon,
}: {
  drafts: ChatChannelInfo[]
  onStart: (type: MarketType) => void
  onContinue: (channel: ChatChannelInfo) => void
  onAbandon: (channel: ChatChannelInfo) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  const [query, setQuery] = useState("")
  const [mode, setMode] = useState<AccessMode>("all")
  const visibleCards = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    return CARDS.filter((card) => {
      const text =
        `${t(`market.cards.${card.type}.name`)} ${t(`market.cards.${card.type}.kind`)}`.toLocaleLowerCase()
      return (mode === "all" || card.mode === mode) && text.includes(normalized)
    })
  }, [mode, query, t])

  return (
    <div className="space-y-4">
      <div className="flex flex-col gap-2 sm:flex-row">
        <div className="relative min-w-0 flex-1">
          <Search className="absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("market.search")}
            className="pl-9"
          />
        </div>
        <Select
          value={mode}
          onValueChange={(value) => setMode(value as AccessMode)}
        >
          <SelectTrigger className="w-full sm:w-48">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {(["all", "scan", "stream", "callback"] as AccessMode[]).map(
              (item) => (
                <SelectItem key={item} value={item}>
                  {t(`market.filters.${item}`)}
                </SelectItem>
              )
            )}
          </SelectContent>
        </Select>
      </div>

      <section className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {visibleCards.map((card) => {
          const pending = drafts.find(
            (draft) => draft.channel_type === card.type
          )
          const config = pending ? parseChannelConfig(pending) : null
          const Icon = card.icon
          return (
            <article
              key={card.type}
              className="flex min-h-48 flex-col rounded-md border bg-card p-4"
            >
              <div className="flex min-w-0 items-start justify-between gap-3">
                <div className="flex min-w-0 flex-1 items-start gap-3">
                  <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border bg-muted/40">
                    <Icon className="h-4 w-4" />
                  </div>
                  <div className="min-w-0">
                    <h3 className="text-sm font-medium leading-5">
                      {t(`market.cards.${card.type}.name`)}
                    </h3>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                      {t(`market.cards.${card.type}.kind`)}
                    </p>
                  </div>
                </div>
                <Badge variant="outline" className="shrink-0 text-[10px]">
                  {t(`market.tags.${card.tag}`)}
                </Badge>
              </div>
              <p className="mt-4 text-xs leading-5 text-muted-foreground">
                {t(`market.cards.${card.type}.description`)}
              </p>
              <div className="mt-auto space-y-2 pt-4">
                {pending ? (
                  <div className="flex gap-2">
                    <Button
                      className="min-w-0 flex-1"
                      onClick={() => onContinue(pending)}
                    >
                      {t("market.continueDraft", {
                        stage: t(
                          `market.states.${config?.setup_state ?? "pending_auth"}`
                        ),
                      })}
                    </Button>
                    <Button
                      variant="outline"
                      size="icon"
                      title={t("market.abandonDraft")}
                      onClick={() => onAbandon(pending)}
                    >
                      <Trash2 className="h-4 w-4 text-destructive" />
                    </Button>
                  </div>
                ) : (
                  <Button
                    variant={
                      card.type === "weixin" || card.type === "wecom_agent"
                        ? "default"
                        : "outline"
                    }
                    className="w-full"
                    onClick={() => onStart(card.type)}
                  >
                    {t(`market.cards.${card.type}.action`)}
                  </Button>
                )}
              </div>
            </article>
          )
        })}
      </section>
      <p className="text-xs text-muted-foreground">{t("market.footerNote")}</p>
    </div>
  )
}

export type { MarketType }
