"use client"

import { useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  downloadWorkspaceFile,
  readWorkspaceFileBase64,
  type TaskArtifactInfo,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { splitAbsPath } from "@/lib/file-open-target"
import { isLocalDesktop } from "@/lib/platform"
import { getShellTransport } from "@/lib/transport"
import { copyTextFromMenu } from "@/lib/utils"

// 与后端受限文件读取的上限保持一致，避免大文件在前端无限占用内存。
const MAX_SAVE_BYTES = 100_000_000

export interface ArtifactCommonActions {
  copyName: () => Promise<void>
  download?: () => Promise<void>
  downloading: boolean
}

export function useArtifactCommonActions(
  artifact: TaskArtifactInfo
): ArtifactCommonActions {
  const t = useTranslations("Folder.taskArtifacts")
  const busy = useRef(false)
  const [downloading, setDownloading] = useState(false)
  const local = artifact.kind !== "url" ? splitAbsPath(artifact.path) : null
  const target = artifact.kind === "file" ? local : null
  const name = local?.ioPath ?? artifact.displayName
  const download = async () => {
    if (!target || busy.current || artifact.status !== "available") return
    busy.current = true
    setDownloading(true)
    try {
      const result = await saveArtifactFile(target)
      if (result === "done") toast.success(t("fileSaved"))
    } catch (error) {
      console.error("[task-artifacts] save failed", {
        artifactId: artifact.id,
        error: toErrorMessage(error),
      })
      toast.error(t("downloadFailed"), { description: toErrorMessage(error) })
    } finally {
      busy.current = false
      setDownloading(false)
    }
  }
  return {
    copyName: async () => {
      const copied = await copyTextFromMenu(name)
      if (copied) toast.success(t("nameCopied"))
      else toast.error(t("copyNameFailed"))
    },
    download: target && artifact.status === "available" ? download : undefined,
    downloading,
  }
}

async function saveArtifactFile(target: {
  rootPath: string
  ioPath: string
}): Promise<"started" | "done" | "cancelled"> {
  if (!isLocalDesktop()) {
    const result = await downloadWorkspaceFile(
      target.rootPath,
      target.ioPath,
      target.ioPath
    )
    return result.status
  }
  const { save } = await import("@tauri-apps/plugin-dialog")
  const path = await save({ defaultPath: target.ioPath })
  if (!path) return "cancelled"
  const dataBase64 = await readWorkspaceFileBase64(
    target.rootPath,
    target.ioPath,
    MAX_SAVE_BYTES
  )
  await getShellTransport().call("save_binary_file", { path, dataBase64 })
  return "done"
}
