"use client"

import { FileText, FolderOpen, Loader2, X } from "lucide-react"
import { useRef, useState, type ChangeEvent } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  readSkillMarketFolder,
  type SelectedSkillMarketFolder,
} from "@/lib/skill-market"

type Translator = (
  key: string,
  values?: Record<string, string | number>
) => string

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

const KNOWN_ERRORS = new Set([
  "emptyFolder",
  "tooManyFiles",
  "invalidFolder",
  "folderTooLarge",
  "missingSkillFile",
  "skillFileTooLarge",
  "emptySkillFile",
  "invalidSkillFile",
  "invalidPath",
  "duplicatePath",
  "pathConflict",
])

function useFolderReader(
  onChange: (folder: SelectedSkillMarketFolder | null) => void
) {
  const [reading, setReading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const handleSelection = async (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.currentTarget.files ?? [])
    event.currentTarget.value = ""
    if (!files.length) return
    setReading(true)
    setError(null)
    try {
      onChange(await readSkillMarketFolder(files))
    } catch (reason) {
      const code = reason instanceof Error ? reason.message : "readFailed"
      setError(KNOWN_ERRORS.has(code) ? code : "readFailed")
      onChange(null)
    } finally {
      setReading(false)
    }
  }
  const clear = () => {
    setError(null)
    onChange(null)
  }
  return { reading, error, handleSelection, clear }
}

type FolderSummaryProps = {
  folder: SelectedSkillMarketFolder
  onClear: () => void
}

function FolderSummary({ folder, onClear }: FolderSummaryProps) {
  const t = useTranslations(
    "SkillsSettings.market.upload"
  ) as unknown as Translator
  return (
    <div className="rounded-md border bg-muted/10">
      <div className="flex min-w-0 items-center gap-2 border-b px-3 py-2">
        <FolderOpen className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-xs font-medium">
          {folder.name}
        </span>
        <span className="shrink-0 text-[11px] text-muted-foreground">
          {t("folderSummary", {
            count: folder.files.length,
            size: formatBytes(folder.totalBytes),
          })}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          title={t("clearFolder")}
          onClick={onClear}
        >
          <X className="size-3.5" />
        </Button>
      </div>
      <div className="max-h-44 overflow-auto p-2">
        {folder.files.map((file) => (
          <div
            key={file.path}
            className="flex min-w-0 items-center gap-2 px-1 py-1 text-[11px]"
          >
            <FileText className="size-3 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 break-all font-mono">
              {file.path}
            </span>
            <span className="shrink-0 text-muted-foreground">
              {formatBytes(file.size)}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

export function SkillMarketFolderPicker({
  folder,
  disabled,
  onChange,
}: {
  folder: SelectedSkillMarketFolder | null
  disabled?: boolean
  onChange: (folder: SelectedSkillMarketFolder | null) => void
}) {
  const t = useTranslations(
    "SkillsSettings.market.upload"
  ) as unknown as Translator
  const inputRef = useRef<HTMLInputElement>(null)
  const reader = useFolderReader(onChange)

  return (
    <div className="space-y-3">
      <input
        {...({ webkitdirectory: "", directory: "" } as Record<string, string>)}
        ref={inputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(event) => void reader.handleSelection(event)}
      />
      <Button
        type="button"
        variant="outline"
        className="w-full rounded-md"
        disabled={disabled || reader.reading}
        onClick={() => inputRef.current?.click()}
      >
        {reader.reading ? (
          <Loader2 className="size-3.5 animate-spin" />
        ) : (
          <FolderOpen className="size-3.5" />
        )}
        {folder ? t("replaceFolder") : t("selectFolder")}
      </Button>
      {reader.error ? (
        <p className="text-xs text-destructive">
          {t(`folderErrors.${reader.error}`)}
        </p>
      ) : null}
      {folder ? <FolderSummary folder={folder} onClear={reader.clear} /> : null}
    </div>
  )
}
