"use client"

import { useMemo, useState } from "react"
import { Upload } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import type { Transport } from "@/lib/transport"
import type { KnowledgeCategory } from "@/lib/cloud-knowledge"
import {
  folderPath,
  refreshLibraryFiles,
  type LibraryFile,
  type LibraryTarget,
} from "@/lib/knowledge-library"
import { knowledgeError } from "@/lib/cloud-knowledge"
import {
  LibraryFolders,
  FolderBreadcrumb,
  MobileFolderPicker,
} from "./library-folders"
import { LibraryFiles } from "./library-files"
import { LibraryError, LibraryLoading } from "./library-states"
import { LibraryUploadDialog } from "./library-upload-dialog"
import { LibraryFileDialogs } from "./library-file-dialogs"
import type { LibraryFileAction } from "./library-file-menu"
import { useLibraryFolders } from "./use-library-folders"

interface BrowserProps {
  transport: Transport
  category: KnowledgeCategory
}
type BrowserModel = ReturnType<typeof useLibraryBrowser>

export function LibraryBrowser(props: BrowserProps) {
  const model = useLibraryBrowser(props)
  const {
    browse,
    uploadTarget,
    target,
    setUploadTarget,
    fileAction,
    selectedFile,
    setFileAction,
  } = model
  if (!browse.data)
    return browse.error ? (
      <LibraryError error={browse.error} retry={browse.reload} />
    ) : (
      <LibraryLoading />
    )
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      {browse.error && (
        <LibraryError error={browse.error} retry={browse.reload} />
      )}
      <div className="flex min-h-0 flex-1">
        <aside className="hidden w-56 shrink-0 flex-col border-r md:flex">
          <LibraryFolders
            folders={browse.data.folders}
            selected={target.folderId}
            onSelect={model.setFolderId}
          />
        </aside>
        <LibraryFolderContent model={model} />
      </div>
      {uploadTarget?.scope === target.scope && (
        <LibraryUploadDialog
          transport={props.transport}
          target={uploadTarget}
          onChanged={browse.reload}
          onClose={() => setUploadTarget(null)}
        />
      )}
      <LibraryFileDialogs
        action={fileAction}
        file={selectedFile}
        target={target}
        folders={browse.data.folders}
        transport={props.transport}
        onClose={() => setFileAction(null)}
        onChanged={browse.reload}
        onOpenPoints={() => setFileAction("points")}
      />
    </div>
  )
}

function useLibraryBrowser(props: BrowserProps) {
  const browse = useLibraryFolders(props.transport, props.category)
  const [folderId, setFolderId] = useState("0")
  const [uploadTarget, setUploadTarget] = useState<LibraryTarget | null>(null)
  const [fileAction, setFileAction] = useState<LibraryFileAction | null>(null)
  const [selectedFile, setSelectedFile] = useState<LibraryFile | null>(null)
  const path = useMemo(
    () => folderPath(browse.data?.folders ?? [], folderId),
    [browse.data?.folders, folderId]
  )
  const effectiveFolderId = path.length
    ? folderId
    : (browse.data?.folders[0]?.folderId ?? folderId)
  const effectivePath = path.length
    ? path
    : folderPath(browse.data?.folders ?? [], effectiveFolderId)
  const folderName = effectivePath
    .map((folder) => folder.folderName)
    .join(" / ")
  const scope = browse.data?.scope ?? ""
  const category = props.category
  const target = useMemo(
    () => ({ scope, category, folderId: effectiveFolderId, folderName }),
    [scope, category, effectiveFolderId, folderName]
  )
  const handleFileAction = async (
    action: LibraryFileAction,
    file: LibraryFile
  ) => {
    setSelectedFile(file)
    if (
      action === "details" ||
      action === "rename" ||
      action === "move" ||
      action === "delete" ||
      action === "points"
    ) {
      setFileAction(action)
      return
    }
    if (action === "view" || action === "download") {
      const url = file.fileUrl || file.tosUrl
      if (!url) return toast.error("文件没有可访问地址")
      window.open(url, "_blank", "noopener,noreferrer")
      return
    }
    if (action === "design") {
      const params = new URLSearchParams({
        agent_id: "1",
        category: String(category),
        file_id: file.fileId,
        file_name: file.fileName,
      })
      window.open(
        `https://ai.iyw.cn/#/agent_new?${params.toString()}`,
        "_blank",
        "noopener,noreferrer"
      )
      return
    }
    if (action === "refresh") {
      try {
        await refreshLibraryFiles(props.transport, {
          scope,
          category,
          fileIds: [file.fileId],
          forceRefresh: true,
        })
        toast.success("文件状态已刷新")
        browse.reload()
      } catch (reason) {
        toast.error(knowledgeError(reason))
      }
    }
  }
  return {
    ...props,
    browse,
    path: effectivePath,
    target,
    fileAction,
    selectedFile,
    setFileAction,
    handleFileAction,
    setFolderId,
    uploadTarget,
    setUploadTarget,
  }
}

function LibraryFolderContent({ model }: { model: BrowserModel }) {
  const t = useTranslations("Library")
  const { browse, target, path, setFolderId } = model
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col p-4 sm:p-6">
      <LibraryFolderActions model={model} />
      <MobileFolderPicker
        folders={browse.data?.folders ?? []}
        selected={target.folderId}
        onSelect={setFolderId}
      />
      {path.length ? (
        <LibraryFiles
          key={`${target.scope}:${target.folderId}`}
          transport={model.transport}
          target={target}
          revision={browse.revision}
          reload={browse.reload}
          onAction={model.handleFileAction}
        />
      ) : (
        <p className="p-4 text-sm text-muted-foreground">{t("emptyLibrary")}</p>
      )}
    </div>
  )
}

function LibraryFolderActions({ model }: { model: BrowserModel }) {
  const t = useTranslations("Library")
  return (
    <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
      <FolderBreadcrumb path={model.path} onSelect={model.setFolderId} />
      <Button
        disabled={!model.path.length}
        onClick={() => model.setUploadTarget(model.target)}
        className="bg-foreground text-background hover:bg-foreground/90"
      >
        <Upload className="size-4" />
        {t("upload")}
      </Button>
    </div>
  )
}
