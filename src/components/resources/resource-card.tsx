"use client"

import { useState } from "react"
import {
  AlertCircle,
  ArrowUpRight,
  ImageIcon,
  MessageSquare,
} from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { artifactVisualKind } from "@/components/layout/task-artifact-type"
import { TaskArtifactTypeIcon } from "@/components/layout/task-artifact-type-icon"
import type { TaskArtifactInfo } from "@/lib/api"
import { buildArtifactThumbnailUrl } from "@/lib/artifact-image-thumbnail"
import { cn } from "@/lib/utils"

const KIND_LABELS = {
  archive: "currentReplyTypeArchive",
  audio: "currentReplyTypeAudio",
  code: "currentReplyTypeCode",
  data: "currentReplyTypeData",
  database: "currentReplyTypeDatabase",
  document: "currentReplyTypeDocument",
  file: "currentReplyTypeFile",
  folder: "currentReplyTypeFolder",
  font: "currentReplyTypeFont",
  image: "currentReplyTypeImage",
  link: "currentReplyTypeLink",
  spreadsheet: "currentReplyTypeSpreadsheet",
  video: "currentReplyTypeVideo",
} as const

const COVER_COLORS = {
  document: "bg-sky-50/70 dark:bg-sky-950/20",
  spreadsheet: "bg-emerald-50/70 dark:bg-emerald-950/20",
  code: "bg-indigo-50/60 dark:bg-indigo-950/20",
  image: "bg-muted/30",
  video: "bg-rose-50/60 dark:bg-rose-950/20",
  audio: "bg-rose-50/60 dark:bg-rose-950/20",
} as const

export function ResourceCard({
  item,
  onSelect,
}: {
  item: TaskArtifactInfo
  onSelect: (item: TaskArtifactInfo) => void
}) {
  return (
    <button
      type="button"
      className="group flex min-w-0 flex-col overflow-hidden rounded-lg border bg-background text-start transition-colors hover:border-foreground/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      onClick={() => onSelect(item)}
      aria-label={item.displayName}
    >
      <ResourceCover item={item} />
      <span className="block w-full min-w-0 p-4">
        <span
          className="line-clamp-2 min-h-10 text-sm font-medium leading-5 break-all"
          title={item.displayName}
        >
          {item.displayName}
        </span>
        <ResourceMetadata item={item} />
      </span>
    </button>
  )
}

function ResourceCover({ item }: { item: TaskArtifactInfo }) {
  const t = useTranslations("Folder.taskArtifacts")
  const kind = artifactVisualKind(item)
  const color = COVER_COLORS[kind as keyof typeof COVER_COLORS] ?? "bg-muted/35"
  return (
    <span
      className={cn(
        "relative grid aspect-[16/9] w-full place-items-center overflow-hidden border-b",
        color
      )}
    >
      <ResourceCoverContent
        key={`${item.path}:${item.lastCheckedAt}:${item.status}`}
        item={item}
      />
      <span className="absolute start-3 bottom-3 rounded-sm border border-border/60 bg-background/95 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
        {t(KIND_LABELS[kind])}
      </span>
      <span className="absolute end-3 top-3 grid size-7 place-items-center rounded-md border bg-background/95 opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
        <ArrowUpRight className="size-3.5" aria-hidden="true" />
      </span>
    </span>
  )
}

function ResourceCoverContent({ item }: { item: TaskArtifactInfo }) {
  const image = artifactVisualKind(item) === "image"
  if (image && item.kind === "url" && item.status === "available") {
    return <ResourceRemoteImage item={item} />
  }
  return (
    <TaskArtifactTypeIcon
      item={item}
      size="md"
      className={cn(
        "[&_svg]:size-9",
        image
          ? "size-full rounded-none bg-transparent bg-contain bg-no-repeat text-muted-foreground shadow-none"
          : "size-16 rounded-lg"
      )}
    />
  )
}

function ResourceRemoteImage({ item }: { item: TaskArtifactInfo }) {
  const [source, setSource] = useState(buildArtifactThumbnailUrl(item.path))
  const [failed, setFailed] = useState(false)
  if (failed) return <ImageIcon className="size-9 text-muted-foreground" />
  return (
    // 成果图片来自运行时地址，保持与现有成果预览相同的加载方式。
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={source}
      alt=""
      loading="lazy"
      decoding="async"
      className="absolute inset-0 size-full object-contain p-2"
      onError={() => {
        if (source !== item.path) setSource(item.path)
        else setFailed(true)
      }}
    />
  )
}

function ResourceMetadata({ item }: { item: TaskArtifactInfo }) {
  const t = useTranslations("Folder.taskArtifacts")
  const r = useTranslations("Resources")
  const locale = useLocale()
  const source = item.conversationTitle || t("untitled")
  const date = new Date(item.createdAt)
  const validDate = !Number.isNaN(date.getTime())
  return (
    <span className="mt-3 block space-y-3 text-xs text-muted-foreground">
      <span
        className="flex min-w-0 items-center gap-1.5 rounded-md border border-emerald-200/70 bg-emerald-50/60 px-2 py-1.5 text-emerald-800 dark:border-emerald-900/70 dark:bg-emerald-950/20 dark:text-emerald-300"
        title={source}
      >
        <MessageSquare className="size-3 shrink-0" aria-hidden="true" />
        <span className="shrink-0 text-[10px] font-normal opacity-70">
          {r("fromConversation")}
        </span>
        <span className="truncate">{source}</span>
      </span>
      <span className="flex min-h-4 flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-3">
        {validDate && (
          <time dateTime={date.toISOString()}>
            {date.toLocaleDateString(locale, {
              year: "numeric",
              month: "short",
              day: "numeric",
            })}
          </time>
        )}
        {item.status !== "available" && (
          <span className="flex items-center gap-1 text-destructive">
            <AlertCircle className="size-3" aria-hidden="true" />
            {t(item.status === "missing" ? "fileMissing" : "fileInaccessible")}
          </span>
        )}
      </span>
    </span>
  )
}
