"use client"

import { Download, ExternalLink, FolderOpen, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { useBrowser } from "@/contexts/browser-context"
import { browserApi } from "@/lib/browser-api"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"

export function BrowserDownloads() {
  const t = useTranslations("Browser")
  const { state, run } = useBrowser()
  const downloads = state?.downloads ?? []
  const active = downloads.filter(
    (item) => item.status === "in_progress"
  ).length

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="relative size-7"
          title={t("downloads")}
          aria-label={t("downloads")}
        >
          <Download className="size-3.5" />
          {active > 0 ? (
            <span className="absolute right-0 top-0 size-1.5 rounded-full bg-primary" />
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 p-0">
        <div className="border-b px-3 py-2 text-xs font-medium">
          {t("downloads")}
        </div>
        <div className="max-h-72 overflow-y-auto">
          {downloads.length === 0 ? (
            <div className="px-3 py-8 text-center text-xs text-muted-foreground">
              {t("noDownloads")}
            </div>
          ) : (
            [...downloads].reverse().map((download) => (
              <div
                key={download.downloadId}
                className="flex items-center gap-2 border-b px-3 py-2 last:border-0"
              >
                <Download className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-xs">
                    {download.suggestedFilename}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {t(`downloadStatus.${download.status}`)}
                  </div>
                </div>
                {download.status === "in_progress" ? (
                  <IconAction
                    label={t("cancelDownload")}
                    icon={X}
                    onClick={() =>
                      void run(() =>
                        browserApi.cancelDownload(download.downloadId)
                      )
                    }
                  />
                ) : download.status === "completed" ? (
                  <>
                    <IconAction
                      label={t("openDownload")}
                      icon={ExternalLink}
                      onClick={() =>
                        void browserApi
                          .openDownload(download.downloadId)
                          .catch(() => {})
                      }
                    />
                    <IconAction
                      label={t("revealDownload")}
                      icon={FolderOpen}
                      onClick={() =>
                        void browserApi
                          .revealDownload(download.downloadId)
                          .catch(() => {})
                      }
                    />
                  </>
                ) : null}
              </div>
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function IconAction({
  label,
  icon: Icon,
  onClick,
}: {
  label: string
  icon: typeof X
  onClick: () => void
}) {
  return (
    <Button
      variant="ghost"
      size="icon"
      className="size-6 shrink-0"
      title={label}
      aria-label={label}
      onClick={onClick}
    >
      <Icon className="size-3.5" />
    </Button>
  )
}
