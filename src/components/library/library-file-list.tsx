"use client"

import { Check, CircleAlert, FileText, Loader2 } from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import {
  isFileProcessing,
  libraryFileSize,
  type LibraryFile,
} from "@/lib/knowledge-library"
import { cn } from "@/lib/utils"

export function LibraryFileList({
  files,
  searching,
}: {
  files: LibraryFile[]
  searching: boolean
}) {
  const t = useTranslations("Library")
  if (!files.length) return <EmptyFiles searching={searching} />
  return (
    <>
      <DesktopFileTable files={files} />
      <ul className="xl:hidden" aria-label={t("files")}>
        {files.map((file) => (
          <MobileFileRow key={file.fileId} file={file} />
        ))}
      </ul>
    </>
  )
}

function EmptyFiles({ searching }: { searching: boolean }) {
  const t = useTranslations("Library")
  return (
    <div className="flex min-h-44 flex-col items-center justify-center gap-2 px-4 text-center">
      <FileText
        className="mb-1 size-6 text-muted-foreground"
        aria-hidden="true"
      />
      <p className="text-sm">{t(searching ? "noMatches" : "emptyFolder")}</p>
      <p className="text-xs text-muted-foreground">
        {t(searching ? "changeSearch" : "uploadFirst")}
      </p>
    </div>
  )
}

function DesktopFileTable({ files }: { files: LibraryFile[] }) {
  const t = useTranslations("Library")
  return (
    <table className="hidden w-full table-fixed text-sm xl:table">
      <caption className="sr-only">{t("files")}</caption>
      <colgroup>
        <col className="w-[31%]" />
        <col className="w-[12%]" />
        <col className="w-[9%]" />
        <col className="w-[14%]" />
        <col className="w-[14%]" />
        <col className="w-[20%]" />
      </colgroup>
      <FileColumnHeaders />
      <tbody>
        {files.map((file) => (
          <DesktopFileRow key={file.fileId} file={file} />
        ))}
      </tbody>
    </table>
  )
}

function FileColumnHeaders() {
  const t = useTranslations("Library")
  return (
    <thead className="sticky top-0 bg-background">
      <tr className="h-10 text-xs font-normal text-muted-foreground">
        {(
          [
            "fileName",
            "size",
            "format",
            "status",
            "uploadedBy",
            "updatedAt",
          ] as const
        ).map((key) => (
          <th
            key={key}
            scope="col"
            className={cn(
              "px-3 text-left font-normal",
              (key === "size" || key === "updatedAt") && "text-right"
            )}
          >
            {t(key)}
          </th>
        ))}
      </tr>
    </thead>
  )
}

function DesktopFileRow({ file }: { file: LibraryFile }) {
  const locale = useLocale()
  const date = fileDate(file.updatedAt, locale)
  return (
    <tr className="h-14 border-t border-border/50 hover:bg-muted/40">
      <td className="px-3">
        <span className="flex min-w-0 items-center gap-2">
          <FileText
            className="size-4 shrink-0 text-muted-foreground"
            aria-hidden="true"
          />
          <span title={file.fileName} className="truncate">
            {file.fileName}
          </span>
        </span>
      </td>
      <td className="px-3 text-right text-xs tabular-nums text-muted-foreground">
        {libraryFileSize(file.fileSize, locale)}
      </td>
      <td className="truncate px-3 text-xs text-muted-foreground">
        {file.fileExt?.toUpperCase() || "—"}
      </td>
      <td className="px-3">
        <LibraryFileStatus file={file} />
      </td>
      <td
        className="truncate px-3 text-xs text-muted-foreground"
        title={file.uploadEmployName}
      >
        {file.uploadEmployName || "—"}
      </td>
      <td className="px-3 text-right text-xs tabular-nums text-muted-foreground">
        <time dateTime={date.iso}>{date.label}</time>
      </td>
    </tr>
  )
}

function MobileFileRow({ file }: { file: LibraryFile }) {
  const locale = useLocale()
  const date = fileDate(file.updatedAt, locale)
  return (
    <li className="min-w-0 space-y-2 border-t border-border/50 px-3 py-4">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <span className="min-w-0 break-words text-sm">{file.fileName}</span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          {libraryFileSize(file.fileSize, locale)}
        </span>
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 text-xs text-muted-foreground">
        <span>{file.fileExt?.toUpperCase() || "—"}</span>
        <LibraryFileStatus file={file} />
        <span className="max-w-full truncate">
          {file.uploadEmployName || "—"}
        </span>
        <time dateTime={date.iso} className="tabular-nums">
          {date.label}
        </time>
      </div>
    </li>
  )
}

function LibraryFileStatus({ file }: { file: LibraryFile }) {
  const t = useTranslations("Library")
  const processing = isFileProcessing(file.docProcessStatus)
  const complete = file.docProcessStatus === 0
  const failed = file.docProcessStatus === 2
  const Icon = processing ? Loader2 : complete ? Check : CircleAlert
  return (
    <span
      title={file.docProcessMessage || undefined}
      className={cn(
        "inline-flex items-center gap-1.5 whitespace-nowrap text-xs",
        failed ? "text-destructive" : "text-muted-foreground"
      )}
    >
      <Icon
        aria-hidden="true"
        className={cn("size-3.5 shrink-0", processing && "animate-spin")}
      />
      {t(
        processing
          ? "processing"
          : complete
            ? "indexed"
            : failed
              ? "processFailed"
              : "unknownStatus"
      )}
    </span>
  )
}

function fileDate(raw: string, locale: string) {
  if (!raw) return { label: "—", iso: undefined }
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return { label: "—", iso: undefined }
  return {
    iso: date.toISOString(),
    label: new Intl.DateTimeFormat(locale, {
      dateStyle: "short",
      timeStyle: "short",
    }).format(date),
  }
}
