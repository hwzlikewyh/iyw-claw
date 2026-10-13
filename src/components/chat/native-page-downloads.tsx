"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { PackageCheck, FolderSearch, FolderOpen } from "lucide-react"
import { getShellTransport } from "@/lib/transport"
import { Button } from "@/components/ui/button"
import type { NativePageDownload } from "@/lib/native-page-api"

export function NativePageDownloads({
  downloads,
}: {
  downloads: NativePageDownload[]
}) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const [open, setOpen] = useState(false)
  if (!downloads.length) return null
  return (
    <div className="shrink-0 border-b text-xs">
      <Button size="sm" variant="ghost" onClick={() => setOpen(!open)}>
        <PackageCheck className="size-3" />
        {t("downloads")} ({downloads.length})
      </Button>
      {open && (
        <div className="max-h-28 overflow-auto px-2 pb-2">
          {downloads.map((download) => (
            <NativeDownloadItem key={download.url} download={download} />
          ))}
        </div>
      )}
    </div>
  )
}

function NativeDownloadItem({ download }: { download: NativePageDownload }) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  return (
    <div key={download.url} className="flex items-center gap-2">
      <span className="min-w-0 flex-1 truncate">
        {download.name ?? download.url}
      </span>
      <span>{t(download.status ?? "started")}</span>
      {download.path && download.status === "completed" && (
        <>
          <Button
            size="icon-xs"
            variant="ghost"
            title={t("openFile")}
            aria-label={t("openFile")}
            onClick={() =>
              void getShellTransport()
                .call("open_local_path", { path: download.path! })
                .catch((error) => toast.error(String(error)))
            }
          >
            <FolderOpen />
          </Button>
          <Button
            size="icon-xs"
            variant="ghost"
            title={t("revealFile")}
            aria-label={t("revealFile")}
            onClick={() =>
              void revealDownload(download.path!).catch((error) =>
                toast.error(String(error))
              )
            }
          >
            <FolderSearch />
          </Button>
        </>
      )}
    </div>
  )
}

async function revealDownload(path: string) {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener")
  await revealItemInDir(path)
}
