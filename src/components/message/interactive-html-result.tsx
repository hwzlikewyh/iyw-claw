"use client"

import { useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { CodeXml } from "lucide-react"
import { Button } from "@/components/ui/button"
import { InteractiveHtmlCard } from "@/components/chat/interactive-html-card"
import type { InteractiveHtmlState } from "@/lib/types"

function parsePage(input?: string | null): InteractiveHtmlState | null {
  try {
    const root = JSON.parse(input ?? "{}")
    const value = root?.arguments ?? root
    if (typeof value?.html !== "string" || typeof value?.title !== "string")
      return null
    return {
      interaction_id: "history-preview",
      title: value.title,
      html: value.html,
      wait_for_response: false,
    }
  } catch {
    return null
  }
}

export function InteractiveHtmlResult({
  input,
  output,
  errorText,
  running,
}: {
  input?: string | null
  output?: string | null
  errorText?: string | null
  running: boolean
}) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  const page = useMemo(() => parsePage(input), [input])
  const [open, setOpen] = useState(false)
  return (
    <div className="space-y-2 rounded-lg border p-3">
      <div className="flex items-center gap-2 text-sm">
        <CodeXml className="size-4" />
        <span className="min-w-0 flex-1 truncate">
          {page?.title ?? t("title")}
        </span>
        {page && !running && (
          <Button size="sm" variant="ghost" onClick={() => setOpen(!open)}>
            {t(open ? "close" : "preview")}
          </Button>
        )}
      </div>
      {running && (
        <p className="text-xs text-muted-foreground">{t("activePage")}</p>
      )}
      {errorText && (
        <p role="alert" className="text-xs text-destructive">
          {errorText}
        </p>
      )}
      {open && page && (
        <>
          <p className="text-xs text-muted-foreground">{t("historyNotice")}</p>
          <InteractiveHtmlCard page={page} preview />
        </>
      )}
      {output && <OutputDetails output={output} />}
    </div>
  )
}

function OutputDetails({ output }: { output: string }) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  return (
    <details className="text-xs text-muted-foreground">
      <summary className="cursor-pointer">{t("result")}</summary>
      <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap">
        {output}
      </pre>
    </details>
  )
}
