"use client"

import { Loader2, RotateCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { LogReportScreenshots } from "./log-report-screenshots"
import { MAX_DESCRIPTION_LENGTH, type LogReportForm } from "./use-log-report"

function SourceInfo({ source }: { source: LogReportForm["source"] }) {
  const t = useTranslations("LogReport")
  return (
    <>
      {source.loading && (
        <p
          className="flex items-center gap-2 text-xs text-muted-foreground"
          role="status"
        >
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("loading")}
        </p>
      )}
      {source.context && (
        <div className="space-y-1 text-xs text-muted-foreground">
          <p>
            {source.context.sourceFiles.length
              ? t("sourceFiles", { count: source.context.sourceFiles.length })
              : t("noLogs")}
          </p>
          <p className="break-all">{source.context.logsDir}</p>
        </div>
      )}
    </>
  )
}

function SourceError({ source }: { source: LogReportForm["source"] }) {
  const t = useTranslations("LogReport")
  if (!source.error) return null
  return (
    <div
      className="flex items-center gap-2 text-sm text-destructive"
      role="alert"
    >
      <span className="min-w-0 flex-1 break-words">{source.error}</span>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        title={t("retry")}
        aria-label={t("retry")}
        onClick={() => void source.load(source.date || undefined)}
      >
        <RotateCw className="h-4 w-4" />
      </Button>
    </div>
  )
}

function ReportDate({ form }: { form: LogReportForm }) {
  const t = useTranslations("LogReport")
  const { source } = form
  return (
    <div className="space-y-2">
      <Label htmlFor="report-date">{t("date")}</Label>
      <Input
        id="report-date"
        type="date"
        className="w-full sm:w-52"
        value={source.date}
        max={source.context?.today}
        disabled={form.busy}
        required
        onChange={(event) => source.changeDate(event.target.value)}
      />
      <SourceInfo source={source} />
      <SourceError source={source} />
    </div>
  )
}

export function LogReportFields({ form }: { form: LogReportForm }) {
  const t = useTranslations("LogReport")
  return (
    <div className="space-y-5">
      <div className="space-y-2">
        <Label htmlFor="report-description">{t("description")}</Label>
        <Textarea
          id="report-description"
          value={form.description}
          onChange={(event) => form.setDescription(event.target.value)}
          placeholder={t("descriptionPlaceholder")}
          maxLength={MAX_DESCRIPTION_LENGTH}
          disabled={form.busy}
          required
          className="min-h-28 resize-y"
        />
        <p className="text-right text-xs tabular-nums text-muted-foreground">
          {form.description.length}/{MAX_DESCRIPTION_LENGTH}
        </p>
      </div>
      <ReportDate form={form} />
      <LogReportScreenshots
        images={form.screenshots.images}
        disabled={form.busy}
        onAdd={form.addScreenshots}
        onRemove={form.screenshots.remove}
      />
      {form.error && (
        <p className="break-words text-sm text-destructive" role="alert">
          {form.error}
        </p>
      )}
    </div>
  )
}
