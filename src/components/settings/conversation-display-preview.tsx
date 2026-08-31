"use client"

import {
  Check,
  CircleAlert,
  ListChecks,
  MessageSquareText,
  Minimize2,
  PanelsTopLeft,
} from "lucide-react"
import { useTranslations } from "next-intl"

import type {
  ConversationDisplayMode,
  ConversationResponseStyle,
} from "@/lib/conversation-display-preferences"
import { cn } from "@/lib/utils"
import { SegmentedControl, SettingRow } from "./settings-ui"
import { Switch } from "@/components/ui/switch"

const RESPONSE_STYLE_OPTIONS: Array<{
  id: ConversationResponseStyle
  icon: typeof Minimize2
}> = [
  { id: "concise", icon: Minimize2 },
  { id: "standard", icon: MessageSquareText },
  { id: "detailed", icon: PanelsTopLeft },
]

interface ConversationDisplayPreviewProps {
  responseStyle: ConversationResponseStyle
  setResponseStyle: (style: ConversationResponseStyle) => void
  processMode: ConversationDisplayMode
  collapseCompletedTurn: boolean
  autoOpenErrors: boolean
  setProcessMode: (mode: ConversationDisplayMode) => void
  setCollapseCompletedTurn: (value: boolean) => void
  setAutoOpenErrors: (value: boolean) => void
}

export function ConversationDisplayPreview({
  responseStyle,
  setResponseStyle,
  processMode,
  collapseCompletedTurn,
  autoOpenErrors,
  setProcessMode,
  setCollapseCompletedTurn,
  setAutoOpenErrors,
}: ConversationDisplayPreviewProps) {
  const t = useTranslations("AppearanceSettings")
  const translate = (key: string) => t(key as never)

  return (
    <div className="space-y-5 p-4">
      <div className="space-y-3">
        <div>
          <div className="text-sm font-medium">
            {t("conversation.responseStyleTitle")}
          </div>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            {t("conversation.responseStyleDescription")}
          </p>
        </div>
        <div className="grid gap-2 sm:grid-cols-3">
          {RESPONSE_STYLE_OPTIONS.map(({ id, icon: Icon }) => {
            const selected = id === responseStyle
            const optionKey = `conversation.responseStyles.${id}`
            return (
              <button
                key={id}
                type="button"
                aria-pressed={selected}
                onClick={() => setResponseStyle(id)}
                className={cn(
                  "relative flex min-h-24 flex-col items-start gap-2 rounded-lg border p-3 text-left transition-colors",
                  selected
                    ? "border-primary bg-primary/[0.06] ring-1 ring-primary/30"
                    : "hover:bg-muted/60"
                )}
              >
                <span
                  className={cn(
                    "flex size-7 items-center justify-center rounded-md",
                    selected
                      ? "bg-primary text-primary-foreground"
                      : "bg-muted text-muted-foreground"
                  )}
                >
                  <Icon className="size-4" aria-hidden="true" />
                </span>
                <span className="min-w-0">
                  <span className="block text-sm font-medium">
                    {translate(`${optionKey}.title`)}
                  </span>
                  <span className="mt-0.5 block text-[11px] leading-4 text-muted-foreground">
                    {translate(`${optionKey}.description`)}
                  </span>
                </span>
                {selected ? (
                  <Check
                    className="absolute right-2.5 top-2.5 size-3.5 text-primary"
                    aria-hidden="true"
                  />
                ) : null}
              </button>
            )
          })}
        </div>
      </div>

      <div className="space-y-4 border-t pt-5">
        <div>
          <div className="text-sm font-medium">
            {t("conversation.processTitle")}
          </div>
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            {t("conversation.processDescription")}
          </p>
        </div>
        <SegmentedControl<ConversationDisplayMode>
          value={processMode}
          onChange={setProcessMode}
          options={[
            {
              value: "minimal",
              label: t("conversation.processConclusionFirst"),
            },
            { value: "summary", label: t("conversation.processSummaryFirst") },
            { value: "full", label: t("conversation.processFullFirst") },
          ]}
        />
        <div className="divide-y rounded-lg border">
          <SettingRow
            title={t("conversation.collapseCompletedTitle")}
            description={t("conversation.collapseCompletedDescription")}
          >
            <Switch
              checked={collapseCompletedTurn}
              onCheckedChange={setCollapseCompletedTurn}
              aria-label={t("conversation.collapseCompletedTitle")}
            />
          </SettingRow>
          <SettingRow
            title={t("conversation.autoOpenErrorsTitle")}
            description={t("conversation.autoOpenErrorsDescription")}
          >
            <Switch
              checked={autoOpenErrors}
              onCheckedChange={setAutoOpenErrors}
              aria-label={t("conversation.autoOpenErrorsTitle")}
            />
          </SettingRow>
        </div>
        <p className="flex items-start gap-1.5 text-[11px] leading-4 text-muted-foreground">
          <CircleAlert className="mt-0.5 size-3 shrink-0" aria-hidden="true" />
          {t("conversation.processSafetyNote")}
        </p>
      </div>

      <div className="flex items-center gap-2 border-t pt-4 text-[11px] text-muted-foreground">
        <ListChecks className="size-3.5" aria-hidden="true" />
        {t("conversation.responseStyleNote")}
      </div>
    </div>
  )
}
