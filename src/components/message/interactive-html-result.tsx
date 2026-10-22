"use client"

import { useMemo } from "react"
import { useTranslations } from "next-intl"
import { CodeXml } from "lucide-react"
import { Button } from "@/components/ui/button"
import { normalizePageUrl } from "@/lib/native-page-api"
import {
  useInteractivePage,
  type InteractivePage,
} from "@/contexts/interactive-page-context"

function parsePage(input?: string | null): InteractivePage | null {
  try {
    const root = JSON.parse(input ?? "{}")
    const value = root?.arguments ?? root
    if (
      (typeof value?.html !== "string" && typeof value?.url !== "string") ||
      typeof value?.title !== "string"
    )
      return null
    if (
      value.url &&
      (!normalizePageUrl(value.url) || typeof value.html === "string")
    )
      return null
    return {
      interaction_id: `history-${crypto.randomUUID()}`,
      title: value.title,
      html: value.html ?? "",
      url: value.url,
      wait_for_response: false,
    }
  } catch {
    return null
  }
}

function interactionId(output?: string | null): string | undefined {
  try {
    return JSON.parse(output ?? "{}").interaction_id
  } catch {
    return undefined
  }
}

export function InteractiveHtmlResult({
  input,
  output,
  running,
}: {
  input?: string | null
  output?: string | null
  errorText?: string | null
  running: boolean
}) {
  const t = useTranslations("Folder.chat.interactiveHtml")
  const page = useMemo(() => parsePage(input), [input])
  const { openPreview, select, pages } = useInteractivePage()
  const liveId = interactionId(output)
  const livePage =
    pages.find((item) => item.interaction_id === liveId) ??
    pages.find(
      (item) =>
        page &&
        item.title === page.title &&
        item.html === page.html &&
        item.url === page.url
    )
  return (
    <div className="space-y-2 rounded-lg border p-3">
      <div className="flex items-center gap-2 text-sm">
        <CodeXml className="size-4" />
        <span className="min-w-0 flex-1 truncate">
          {page?.title ?? t("title")}
        </span>
        {page && (
          <Button
            size="sm"
            variant="ghost"
            onClick={() =>
              livePage ? select(livePage.interaction_id) : openPreview(page)
            }
          >
            {t("preview")}
          </Button>
        )}
      </div>
      {running && (
        <p className="text-xs text-muted-foreground">{t("activePage")}</p>
      )}
    </div>
  )
}
