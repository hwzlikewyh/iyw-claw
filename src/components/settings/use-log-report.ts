"use client"

import { useCallback, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import {
  toLocalizedErrorMessage,
  type AppErrorTranslator,
} from "@/lib/app-error"
import {
  encodeReportScreenshot,
  getLogReportContext,
  submitLogReport,
  type LogReportContext,
  type LogReportResult,
} from "@/lib/log-report"
import { useReportScreenshots } from "./use-report-screenshots"

export const MAX_DESCRIPTION_LENGTH = 2000

interface SourceState {
  context: LogReportContext | null
  date: string
  loading: boolean
  error: string
}

const EMPTY_SOURCE: SourceState = {
  context: null,
  date: "",
  loading: false,
  error: "",
}

function useReportContext(localize: (error: unknown) => string) {
  const [state, setState] = useState(EMPTY_SOURCE)
  const generation = useRef(0)
  const load = useCallback(
    async (selected?: string) => {
      const version = ++generation.current
      setState((previous) => ({
        context: null,
        date: selected ?? previous.date,
        error: "",
        loading: true,
      }))
      try {
        const result = await getLogReportContext(selected)
        if (version !== generation.current) return
        setState({
          context: result,
          date: result.date,
          error: "",
          loading: false,
        })
      } catch (error) {
        if (version === generation.current)
          setState((previous) => ({
            ...previous,
            loading: false,
            error: localize(error),
          }))
      }
    },
    [localize]
  )
  const changeDate = (value: string) => {
    if (value) void load(value)
    else {
      generation.current++
      setState(EMPTY_SOURCE)
    }
  }
  return {
    ...state,
    load,
    changeDate,
    resetDate: () => setState((previous) => ({ ...previous, date: "" })),
  }
}

interface ReportDraft {
  source: ReturnType<typeof useReportContext>
  screenshots: ReturnType<typeof useReportScreenshots>
  description: string
  setDescription: (description: string) => void
}

async function sendDraft(draft: ReportDraft) {
  const images = await Promise.all(
    draft.screenshots.images.map((image) => encodeReportScreenshot(image.file))
  )
  return submitLogReport({
    date: draft.source.context!.date,
    description: draft.description.trim(),
    screenshots: images,
  })
}

function useSubmission(
  draft: ReportDraft,
  localize: (error: unknown) => string
) {
  const [busy, setBusy] = useState(false)
  const submitting = useRef(false)
  const [error, setError] = useState("")
  const [result, setResult] = useState<LogReportResult | null>(null)
  const submit = async () => {
    if (
      submitting.current ||
      !draft.source.context ||
      !draft.description.trim() ||
      draft.description.length > MAX_DESCRIPTION_LENGTH
    )
      return
    submitting.current = true
    setBusy(true)
    setError("")
    try {
      setResult(await sendDraft(draft))
      draft.setDescription("")
      draft.screenshots.clear()
      draft.source.resetDate()
    } catch (error) {
      setError(localize(error))
    } finally {
      submitting.current = false
      setBusy(false)
    }
  }
  return {
    busy,
    error,
    result,
    submit,
    submitting,
    reset: () => {
      setResult(null)
      setError("")
    },
  }
}

export function useLogReport() {
  const t = useTranslations("LogReport")
  const root = useTranslations()
  const localize = useCallback(
    (error: unknown) =>
      toLocalizedErrorMessage(error, root as unknown as AppErrorTranslator),
    [root]
  )
  const source = useReportContext(localize)
  const screenshots = useReportScreenshots()
  const [open, setOpen] = useState(false)
  const [description, setDescription] = useState("")
  const draft = { source, screenshots, description, setDescription }
  const submission = useSubmission(draft, localize)
  const changeOpen = (value: boolean) => {
    if (submission.submitting.current) return
    setOpen(value)
    if (value) {
      submission.reset()
      void source.load(source.date || undefined)
    }
  }
  const addScreenshots = (files: File[]) => {
    if (!submission.submitting.current && !screenshots.add(files))
      toast.error(t("errors.screenshots"))
  }
  return {
    open,
    changeOpen,
    ...draft,
    addScreenshots,
    ...submission,
  }
}

export type LogReportForm = ReturnType<typeof useLogReport>
