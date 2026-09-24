"use client"

import Image from "next/image"
import { Check, Loader2, RotateCw, TriangleAlert } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"
import { StartupFailureActions } from "./startup-runtime-status"

const SETUP_STAGES = ["prepare", "initialize", "finish"] as const

interface InitializationPanelProps {
  stage: number
  percent: number | null
  waiting: boolean
  failure: string | null
  onRetry: () => void
}

function InitializationSteps({ stage }: { stage: number }) {
  const t = useTranslations("StartupCodex")
  return (
    <ol className="grid grid-cols-3 gap-4 text-xs">
      {SETUP_STAGES.map((step, index) => (
        <li key={step} aria-current={stage === index ? "step" : undefined}>
          <div
            className={cn(
              "flex min-h-10 items-start gap-2",
              stage < index && "text-muted-foreground"
            )}
          >
            {stage > index ? (
              <Check className="size-4 shrink-0 text-teal-600 dark:text-teal-400" />
            ) : (
              <span className="tabular-nums text-muted-foreground">
                0{index + 1}
              </span>
            )}
            <span className="min-w-0 break-words">
              {t(`setupStages.${step}`)}
            </span>
          </div>
          <div
            className={cn(
              "h-0.5 bg-border",
              stage >= index && "bg-teal-600 dark:bg-teal-400"
            )}
          />
        </li>
      ))}
    </ol>
  )
}

function InitializationProgress({
  percent,
  label,
}: {
  percent: number | null
  label: string
}) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
      className="h-2 overflow-hidden bg-muted"
    >
      <div
        className={cn(
          "h-full bg-teal-600 transition-[width] duration-300 motion-reduce:transition-none dark:bg-teal-400",
          percent === null && "animate-pulse motion-reduce:animate-none"
        )}
        style={{ width: percent === null ? "30%" : `${percent}%` }}
      />
    </div>
  )
}

function InitializationBrand() {
  const t = useTranslations("StartupCodex")
  return (
    <header className="flex items-center justify-between gap-4 border-b px-7 py-5">
      <div className="flex min-w-0 items-center gap-3">
        <Image
          src="/icon-128x128.png"
          alt=""
          width={36}
          height={36}
          className="shrink-0"
          unoptimized
        />
        <span className="text-lg font-semibold">{t("setupBrand")}</span>
      </div>
      <span className="text-xs text-muted-foreground">{t("setupLabel")}</span>
    </header>
  )
}

function InitializationStatus({
  percent,
  failure,
  description,
}: {
  percent: number | null
  failure: string | null
  description: string
}) {
  const t = useTranslations("StartupCodex")
  return (
    <DialogHeader className="gap-3 text-start">
      <DialogTitle className="flex items-center gap-2 text-xl">
        {failure ? <TriangleAlert className="size-5 text-destructive" /> : null}
        {t(failure ? "setupFailed" : "setupTitle")}
      </DialogTitle>
      <div className="flex min-h-11 items-center justify-between gap-4">
        <DialogDescription className="text-sm">{description}</DialogDescription>
        {!failure &&
          (percent === null ? (
            <Loader2
              aria-label={t("setupTitle")}
              className="size-6 shrink-0 animate-spin text-teal-600 motion-reduce:animate-none"
            />
          ) : (
            <span className="shrink-0 text-3xl font-medium text-teal-700 tabular-nums dark:text-teal-400">
              {percent}%
            </span>
          ))}
      </div>
    </DialogHeader>
  )
}

export function StartupInitializationPanel({
  stage,
  percent,
  waiting,
  failure,
  onRetry,
}: InitializationPanelProps) {
  const t = useTranslations("StartupCodex")
  const label = t(`setupStages.${SETUP_STAGES[stage]}`)
  return (
    <>
      <InitializationBrand />
      <div className="grid gap-8 p-7">
        <InitializationSteps stage={stage} />
        <InitializationStatus
          percent={percent}
          failure={failure}
          description={waiting ? t("waitingForWriter") : label}
        />
        {failure ? (
          <InitializationFailure detail={failure} onRetry={onRetry} />
        ) : (
          <InitializationProgress percent={percent} label={label} />
        )}
      </div>
    </>
  )
}

function InitializationFailure({
  detail,
  onRetry,
}: {
  detail: string
  onRetry: () => void
}) {
  const t = useTranslations("StartupCodex")
  return (
    <div className="grid gap-4">
      <p className="text-sm text-muted-foreground">{t("setupFailureHint")}</p>
      <Button className="justify-self-start" onClick={onRetry}>
        <RotateCw className="size-4" />
        {t("retry")}
      </Button>
      <details className="min-w-0 text-xs text-muted-foreground">
        <summary className="cursor-pointer py-2">{t("setupDetails")}</summary>
        <pre className="my-3 max-h-36 overflow-auto break-all whitespace-pre-wrap">
          {detail}
        </pre>
        <StartupFailureActions detail={detail} />
      </details>
    </div>
  )
}
