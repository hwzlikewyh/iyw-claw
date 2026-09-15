"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { ChevronLeft, ChevronRight, LockKeyhole } from "lucide-react"
import { PreviewAction } from "./preview-action"

export function PdfNavigation({
  page,
  pages,
  onPage,
}: {
  page: number
  pages: number
  onPage: (value: number) => void
}) {
  const t = useTranslations("Folder.fileWorkspacePanel")
  const labels = useTranslations("Folder.previewControls")
  return (
    <>
      <PreviewAction
        label={t("prev")}
        disabled={page <= 1}
        onClick={() => onPage(page - 1)}
      >
        <ChevronLeft className="size-4" />
      </PreviewAction>
      <input
        aria-label={labels("page")}
        type="number"
        step={1}
        className="h-7 w-14 bg-background text-center text-xs"
        min={1}
        max={pages || 1}
        value={page}
        onChange={(event) =>
          onPage(
            Math.max(
              1,
              Math.min(pages || 1, Math.floor(Number(event.target.value)) || 1)
            )
          )
        }
      />
      <span className="text-xs tabular-nums">/ {pages || "..."}</span>
      <PreviewAction
        label={t("next")}
        disabled={!pages || page >= pages}
        onClick={() => onPage(page + 1)}
      >
        <ChevronRight className="size-4" />
      </PreviewAction>
    </>
  )
}

export function PdfPassword({
  onSubmit,
}: {
  onSubmit: (password: string) => void
}) {
  const [password, setPassword] = useState("")
  const t = useTranslations("Folder.previewControls")
  return (
    <form
      className="flex items-center gap-2 p-3"
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit(password)
        setPassword("")
      }}
    >
      <LockKeyhole className="size-4 shrink-0" />
      <input
        type="password"
        autoComplete="off"
        aria-label={t("password")}
        value={password}
        onChange={(event) => setPassword(event.target.value)}
        className="min-w-0 border px-2 py-1"
      />
      <PreviewAction type="submit" label={t("unlock")}>
        <ChevronRight className="size-4" />
      </PreviewAction>
    </form>
  )
}
