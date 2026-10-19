"use client"

import { useId } from "react"
import { Check, LoaderCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import type {
  ConversationDisplayMode,
  ConversationResponseStyle,
} from "@/lib/conversation-display-preferences"
import { SettingRow } from "./settings-ui"
import { Switch } from "@/components/ui/switch"

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

export function ConversationDisplayPreview(
  props: ConversationDisplayPreviewProps
) {
  const t = useTranslations("AppearanceSettings")
  return (
    <div className="space-y-5 p-4">
      <ResponseChoices {...props} />
      <ProcessChoices {...props} />
      <DisplayExample {...props} />
      <ProcessBehavior {...props} />
      <p className="text-xs leading-5 text-muted-foreground">
        {t("conversation.processSafetyNote")}
      </p>
    </div>
  )
}

function ResponseChoices(props: ConversationDisplayPreviewProps) {
  const t = useTranslations("AppearanceSettings")
  const group = useId()
  const options: ConversationResponseStyle[] = [
    "concise",
    "standard",
    "detailed",
  ]
  return (
    <fieldset className="min-w-0 space-y-3">
      <legend className="text-sm font-medium">
        {t("conversation.responseStyleTitle")}
      </legend>
      <div className="grid grid-cols-3 gap-1 rounded-lg border bg-muted/50 p-1">
        {options.map((style) => (
          <label key={style} className="min-w-0 cursor-pointer">
            <input
              className="peer sr-only"
              type="radio"
              name={group}
              checked={props.responseStyle === style}
              onChange={() => props.setResponseStyle(style)}
            />
            <span className="block rounded-md px-2 py-2 text-center text-sm text-muted-foreground peer-checked:bg-background peer-checked:text-foreground peer-checked:shadow-sm peer-focus-visible:ring-2 peer-focus-visible:ring-ring">
              {t(`conversation.responseStyles.${style}.title`)}
            </span>
          </label>
        ))}
      </div>
      <p className="text-xs leading-5 text-muted-foreground">
        {t("conversation.responseStyleDescription")}
      </p>
    </fieldset>
  )
}

const PROCESS_LABELS = {
  minimal: "processMinimal",
  summary: "processSummary",
  full: "processFull",
} as const

function ProcessChoices(props: ConversationDisplayPreviewProps) {
  const t = useTranslations("AppearanceSettings")
  const p = useTranslations("ConversationPresentation")
  const group = useId()
  const options: ConversationDisplayMode[] = ["minimal", "summary", "full"]
  return (
    <fieldset className="min-w-0 space-y-3 border-t pt-4">
      <legend className="text-sm font-medium">
        {t("conversation.processTitle")}
      </legend>
      <div className="space-y-1">
        {options.map((mode) => (
          <label
            key={mode}
            className="flex min-h-9 cursor-pointer items-center gap-3 text-sm"
          >
            <input
              className="size-3.5 shrink-0 accent-primary"
              type="radio"
              name={group}
              checked={props.processMode === mode}
              onChange={() => props.setProcessMode(mode)}
            />
            {p(PROCESS_LABELS[mode])}
          </label>
        ))}
      </div>
      <p className="text-xs leading-5 text-muted-foreground">
        {t("conversation.processDescription")}
      </p>
    </fieldset>
  )
}

function DisplayExample(props: ConversationDisplayPreviewProps) {
  const p = useTranslations("ConversationPresentation")
  return (
    <div
      className="space-y-3 rounded-lg border bg-muted/20 p-4"
      aria-label={p("previewTitle")}
    >
      <p className="text-[11px] text-muted-foreground">{p("previewTitle")}</p>
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Check aria-hidden="true" className="size-3" />
        {props.processMode === "minimal"
          ? p("processRecord")
          : p("completedOperations", { count: 2 })}
      </div>
      {props.processMode === "full" && (
        <div aria-hidden="true" className="space-y-2 border-s ps-4">
          <div className="h-1.5 w-3/4 rounded bg-muted" />
          <div className="h-1.5 w-1/2 rounded bg-muted" />
        </div>
      )}
      <div className="flex items-center gap-2 text-sm font-medium text-primary">
        <LoaderCircle aria-hidden="true" className="size-3.5" />
        {p("previewActivity")}
      </div>
      <p className="text-sm leading-7">
        {p(`previewResponse.${props.responseStyle}`)}
      </p>
    </div>
  )
}

function ProcessBehavior(props: ConversationDisplayPreviewProps) {
  const t = useTranslations("AppearanceSettings")
  return (
    <div className="divide-y border-t">
      <SettingRow
        title={t("conversation.collapseCompletedTitle")}
        description={t("conversation.collapseCompletedDescription")}
      >
        <Switch
          checked={props.collapseCompletedTurn}
          onCheckedChange={props.setCollapseCompletedTurn}
          aria-label={t("conversation.collapseCompletedTitle")}
        />
      </SettingRow>
      <SettingRow
        title={t("conversation.autoOpenErrorsTitle")}
        description={t("conversation.autoOpenErrorsDescription")}
      >
        <Switch
          checked={props.autoOpenErrors}
          onCheckedChange={props.setAutoOpenErrors}
          aria-label={t("conversation.autoOpenErrorsTitle")}
        />
      </SettingRow>
    </div>
  )
}
