"use client"

import { CheckCircle2, Copy, Loader2, Upload } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { LogReportResult } from "@/lib/log-report"
import { copyTextToClipboard } from "@/lib/utils"
import { filesFromClipboard } from "@/lib/clipboard-images"
import { LogReportFields } from "./log-report-fields"
import { useLogReport, type LogReportForm } from "./use-log-report"

function ReportLocation({ result }: { result: LogReportResult }) {
  const t = useTranslations("LogReport")
  const copy = async () => {
    try {
      const copied = await copyTextToClipboard(
        `${result.date}\n${result.fileUrl}`
      )
      if (copied) toast.success(t("copied"))
      else toast.error(t("copyFailed"))
    } catch {
      toast.error(t("copyFailed"))
    }
  }
  return (
    <div className="flex items-start gap-2">
      <p className="min-w-0 flex-1 break-all text-xs text-muted-foreground">
        {result.fileUrl}
      </p>
      <Button
        type="button"
        size="icon"
        variant="outline"
        className="shrink-0"
        title={t("copyReport")}
        aria-label={t("copyReport")}
        onClick={() => void copy()}
      >
        <Copy className="h-4 w-4" />
      </Button>
    </div>
  )
}

function ReportSuccess({ result }: { result: LogReportResult }) {
  const t = useTranslations("LogReport")
  return (
    <div className="space-y-4">
      <p className="flex items-center gap-2 font-medium">
        <CheckCircle2 className="h-5 w-5 text-emerald-600" />
        {t("success")}
      </p>
      <p className="text-sm text-muted-foreground">
        {t("result", { date: result.date, count: result.logRecords })}
      </p>
      {result.skippedRecords > 0 && (
        <p className="text-sm text-amber-700 dark:text-amber-400">
          {t("skipped", { count: result.skippedRecords })}
        </p>
      )}
      <ReportLocation result={result} />
    </div>
  )
}

function ReportActions({ form }: { form: LogReportForm }) {
  const t = useTranslations("LogReport")
  return (
    <DialogFooter>
      <Button
        type="button"
        variant="outline"
        disabled={form.busy}
        onClick={() => form.changeOpen(false)}
      >
        {form.result ? t("close") : t("cancel")}
      </Button>
      {!form.result && (
        <Button
          type="submit"
          disabled={
            form.busy || !form.source.context || !form.description.trim()
          }
        >
          {form.busy ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Upload className="h-4 w-4" />
          )}
          {form.busy ? t("submitting") : t("submit")}
        </Button>
      )}
    </DialogFooter>
  )
}

function ReportDialog({ form }: { form: LogReportForm }) {
  const t = useTranslations("LogReport")
  return (
    <Dialog open={form.open} onOpenChange={form.changeOpen}>
      <DialogContent
        className="max-w-xl rounded-lg"
        showCloseButton={!form.busy}
        onPaste={(event) => {
          const files = filesFromClipboard(event.clipboardData)
          if (files.length && !form.result) {
            event.preventDefault()
            form.addScreenshots(files)
          }
        }}
      >
        <DialogHeader>
          <DialogTitle>{t("title")}</DialogTitle>
          <DialogDescription className="sr-only">
            {t("title")}
          </DialogDescription>
        </DialogHeader>
        <form
          className="min-w-0 space-y-5"
          onSubmit={(event) => {
            event.preventDefault()
            void form.submit()
          }}
        >
          {form.result ? (
            <ReportSuccess result={form.result} />
          ) : (
            <LogReportFields form={form} />
          )}
          <ReportActions form={form} />
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function LogReportSettings() {
  const t = useTranslations("LogReport")
  const form = useLogReport()
  return (
    <section className="flex flex-wrap items-center justify-between gap-3 border-b py-4">
      <h2 className="text-sm font-semibold">{t("sectionTitle")}</h2>
      <Button variant="outline" size="sm" onClick={() => form.changeOpen(true)}>
        <Upload className="h-4 w-4" />
        {t("title")}
      </Button>
      <ReportDialog form={form} />
    </section>
  )
}
