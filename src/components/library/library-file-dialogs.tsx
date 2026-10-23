"use client"

import { useMemo, useState } from "react"
import { Loader2 } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  deleteLibraryFile,
  updateLibraryFile,
  type LibraryFile,
  type LibraryTarget,
} from "@/lib/knowledge-library"
import { knowledgeError } from "@/lib/cloud-knowledge"
import type { KnowledgeFolder } from "@/lib/cloud-knowledge"
import { LibraryPointsDialog } from "./library-points-dialog"
import type { LibraryFileAction } from "./library-file-menu"

export function LibraryFileDialogs({
  action,
  file,
  target,
  folders,
  transport,
  onClose,
  onChanged,
  onOpenPoints,
}: {
  action: LibraryFileAction | null
  file: LibraryFile | null
  target: LibraryTarget
  folders: KnowledgeFolder[]
  transport: Parameters<typeof updateLibraryFile>[0]
  onClose: () => void
  onChanged: () => void
  onOpenPoints: () => void
}) {
  if (!action || !file) return null
  if (action === "points")
    return (
      <LibraryPointsDialog
        file={file}
        target={target}
        transport={transport}
        onClose={onClose}
      />
    )
  if (action === "details")
    return (
      <FileDetailsDialog
        file={file}
        onClose={onClose}
        onPoints={onOpenPoints}
      />
    )
  if (action === "rename")
    return (
      <RenameDialog
        file={file}
        target={target}
        transport={transport}
        onClose={onClose}
        onChanged={onChanged}
      />
    )
  if (action === "move")
    return (
      <MoveDialog
        file={file}
        target={target}
        folders={folders}
        transport={transport}
        onClose={onClose}
        onChanged={onChanged}
      />
    )
  if (action === "delete")
    return (
      <DeleteDialog
        file={file}
        target={target}
        transport={transport}
        onClose={onClose}
        onChanged={onChanged}
      />
    )
  return null
}

export function FileDetailsDialog({
  file,
  onClose,
  onPoints,
}: {
  file: LibraryFile
  onClose: () => void
  onPoints: () => void
}) {
  const t = useTranslations("Library")
  const locale = useLocale()
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="max-w-xl rounded-lg p-5">
        <DialogHeader>
          <DialogTitle>{t("details")}</DialogTitle>
          <DialogDescription className="truncate">
            {file.fileName}
          </DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 text-sm">
          <Detail label={t("fileName")} value={file.fileName} />
          <Detail
            label={t("size")}
            value={new Intl.NumberFormat(locale).format(file.fileSize)}
          />
          <Detail
            label={t("format")}
            value={file.fileExt?.toUpperCase() || "—"}
          />
          <Detail
            label={t("status")}
            value={file.docProcessMessage || t("unknownStatus")}
          />
          <Detail label={t("syncStatus")} value={file.syncMessage || "—"} />
          <Detail label={t("folder")} value={file.folderId} />
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("close")}
          </Button>
          <Button onClick={onPoints}>{t("sliceManagement")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4 border-b border-border/50 pb-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="max-w-[70%] break-words text-right">{value}</span>
    </div>
  )
}

function RenameDialog({
  file,
  target,
  transport,
  onClose,
  onChanged,
}: {
  file: LibraryFile
  target: LibraryTarget
  transport: Parameters<typeof updateLibraryFile>[0]
  onClose: () => void
  onChanged: () => void
}) {
  const t = useTranslations("Library")
  const [name, setName] = useState(file.fileName)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const save = async () => {
    const next = name.trim()
    if (!next || next === file.fileName) return onClose()
    setBusy(true)
    setError("")
    try {
      await updateLibraryFile(transport, {
        ...target,
        fileId: file.fileId,
        fileName: next,
      })
      onChanged()
      onClose()
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="max-w-md rounded-lg p-5">
        <DialogHeader>
          <DialogTitle>{t("rename")}</DialogTitle>
        </DialogHeader>
        <Input
          value={name}
          onChange={(event) => setName(event.target.value)}
          maxLength={180}
          autoFocus
        />
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            {t("cancel")}
          </Button>
          <Button onClick={() => void save()} disabled={busy || !name.trim()}>
            {busy && <Loader2 className="size-4 animate-spin" />}
            {t("save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function MoveDialog({
  file,
  target,
  folders,
  transport,
  onClose,
  onChanged,
}: {
  file: LibraryFile
  target: LibraryTarget
  folders: KnowledgeFolder[]
  transport: Parameters<typeof updateLibraryFile>[0]
  onClose: () => void
  onChanged: () => void
}) {
  const t = useTranslations("Library")
  const options = useMemo(() => flattenFolders(folders), [folders])
  const [folderId, setFolderId] = useState(file.folderId)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const save = async () => {
    if (!folderId || folderId === file.folderId) return onClose()
    setBusy(true)
    setError("")
    try {
      await updateLibraryFile(transport, {
        ...target,
        fileId: file.fileId,
        destinationFolderId: folderId,
      })
      onChanged()
      onClose()
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="max-w-md rounded-lg p-5">
        <DialogHeader>
          <DialogTitle>{t("moveTo")}</DialogTitle>
          <DialogDescription className="truncate">
            {file.fileName}
          </DialogDescription>
        </DialogHeader>
        <select
          className="h-9 rounded-md border bg-background px-3 text-sm"
          value={folderId}
          onChange={(event) => setFolderId(event.target.value)}
        >
          {options.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
        </select>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            {t("cancel")}
          </Button>
          <Button onClick={() => void save()} disabled={busy}>
            {busy && <Loader2 className="size-4 animate-spin" />}
            {t("save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function DeleteDialog({
  file,
  target,
  transport,
  onClose,
  onChanged,
}: {
  file: LibraryFile
  target: LibraryTarget
  transport: Parameters<typeof deleteLibraryFile>[0]
  onClose: () => void
  onChanged: () => void
}) {
  const t = useTranslations("Library")
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState("")
  const remove = async () => {
    setBusy(true)
    setError("")
    try {
      await deleteLibraryFile(transport, {
        scope: target.scope,
        category: target.category,
        fileId: file.fileId,
      })
      onChanged()
      onClose()
    } catch (reason) {
      setError(knowledgeError(reason))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="max-w-md rounded-lg p-5">
        <DialogHeader>
          <DialogTitle>{t("delete")}</DialogTitle>
          <DialogDescription>
            {t("deleteConfirm", { name: file.fileName })}
          </DialogDescription>
        </DialogHeader>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            {t("cancel")}
          </Button>
          <Button
            variant="destructive"
            onClick={() => void remove()}
            disabled={busy}
          >
            {busy && <Loader2 className="size-4 animate-spin" />}
            {t("delete")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function flattenFolders(
  folders: KnowledgeFolder[],
  level = 0
): { id: string; label: string }[] {
  return folders.flatMap((folder) => [
    { id: folder.folderId, label: `${"　".repeat(level)}${folder.folderName}` },
    ...flattenFolders(folder.children, level + 1),
  ])
}
