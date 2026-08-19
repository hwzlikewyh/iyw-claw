"use client"

import {
  CheckCircle2,
  CircleAlert,
  Copy,
  ExternalLink,
  Loader2,
} from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { openUrl } from "@/lib/platform"
import { copyTextToClipboard } from "@/lib/utils"

export type WecomAgentStep = 1 | 2 | 3 | 4

const ADMIN_URL = "https://work.weixin.qq.com/wework_admin/frame"

export function normalizeHttpsUrl(value: string) {
  try {
    const url = new URL(value.trim())
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    ) {
      return null
    }
    return url.toString().replace(/\/$/, "")
  } catch {
    return null
  }
}

export function WecomAgentStepper({ step }: { step: WecomAgentStep }) {
  const t = useTranslations("ChatChannelSettings")
  const steps: WecomAgentStep[] = [1, 2, 3, 4]
  return (
    <div className="grid min-w-0 grid-cols-2 gap-px overflow-hidden rounded-md border bg-border sm:grid-cols-4">
      {steps.map((item) => (
        <div
          key={item}
          className={`min-w-0 bg-background p-2.5 text-xs ${
            item === step
              ? "bg-primary/10 text-primary"
              : "text-muted-foreground"
          }`}
        >
          <strong className="block truncate">
            {t(`market.wecomAgent.steps.${item}`)}
          </strong>
        </div>
      ))}
    </div>
  )
}

export function WecomAgentGuide() {
  const t = useTranslations("ChatChannelSettings")
  const guideKeys = ["corp", "agent", "callback"] as const
  return (
    <section className="min-w-0 space-y-3 border-b pb-5 md:border-r md:border-b-0 md:pr-5 md:pb-0">
      <h3 className="text-sm font-medium">
        {t("market.wecomAgent.guideTitle")}
      </h3>
      {guideKeys.map((key, index) => (
        <div
          key={key}
          className="flex min-w-0 gap-2.5 text-xs text-muted-foreground"
        >
          <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary">
            {index + 1}
          </span>
          <span className="min-w-0">{t(`market.wecomAgent.guide.${key}`)}</span>
        </div>
      ))}
      <div className="rounded-md border border-amber-500/30 bg-amber-500/5 p-3 text-xs text-amber-700 dark:text-amber-300">
        {t("market.wecomAgent.scopeWarning")}
      </div>
      <Button
        variant="outline"
        size="sm"
        onClick={() => void openUrl(ADMIN_URL)}
      >
        <ExternalLink className="h-3.5 w-3.5" />
        {t("market.openAdmin")}
      </Button>
    </section>
  )
}

export function SetupField({
  label,
  value,
  onChange,
  secret = false,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  secret?: boolean
}) {
  return (
    <div className="min-w-0 space-y-1.5">
      <label className="text-xs font-medium">{label}</label>
      <Input
        type={secret ? "password" : "text"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        autoComplete="off"
      />
    </div>
  )
}

export function CopyField({ label, value }: { label: string; value: string }) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium">{label}</label>
      <div className="flex min-w-0 items-center gap-1 rounded-md border bg-muted/30 p-1 pl-3">
        <code className="min-w-0 flex-1 break-all text-xs">{value}</code>
        <Button
          variant="ghost"
          size="icon-sm"
          title={t("market.copy")}
          onClick={() => void copyTextToClipboard(value)}
        >
          <Copy className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  )
}

export function SetupStatus({
  ok,
  pending = false,
  text,
}: {
  ok: boolean
  pending?: boolean
  text: string
}) {
  const Icon = ok ? CheckCircle2 : pending ? Loader2 : CircleAlert
  return (
    <div
      className={`flex items-center gap-2 rounded-md border p-3 text-xs ${
        ok
          ? "border-green-500/30 bg-green-500/5 text-green-700 dark:text-green-300"
          : "text-muted-foreground"
      }`}
    >
      <Icon className={`h-4 w-4 shrink-0 ${pending ? "animate-spin" : ""}`} />
      {text}
    </div>
  )
}

export function WizardActions({
  loading,
  nextDisabled = false,
  onCancel,
  onNext,
}: {
  loading: boolean
  nextDisabled?: boolean
  onCancel: () => void
  onNext: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="flex flex-col-reverse gap-2 pt-2 sm:flex-row sm:justify-end">
      <Button variant="outline" onClick={onCancel} disabled={loading}>
        {t("market.keepDraft")}
      </Button>
      <Button disabled={loading || nextDisabled} onClick={onNext}>
        {loading && <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" />}
        {t("market.continue")}
      </Button>
    </div>
  )
}

export function ErrorBox({ message }: { message: string }) {
  return (
    <div className="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {message}
    </div>
  )
}
