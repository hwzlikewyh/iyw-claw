"use client"

import { Loader2, RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"

export function LibraryLoading() {
  const t = useTranslations("Library")
  return (
    <p
      role="status"
      className="flex items-center gap-2 p-6 text-sm text-muted-foreground"
    >
      <Loader2 className="size-4 animate-spin" aria-hidden="true" />
      {t("loading")}
    </p>
  )
}

export function LibraryError({
  error,
  retry,
}: {
  error: string
  retry: () => void
}) {
  const t = useTranslations("Library")
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-3 rounded-md border p-4 text-sm"
    >
      <p className="min-w-0 flex-1 break-words">{error}</p>
      <Button variant="outline" size="sm" onClick={retry}>
        <RefreshCw className="size-3.5" />
        {t("retry")}
      </Button>
    </div>
  )
}
