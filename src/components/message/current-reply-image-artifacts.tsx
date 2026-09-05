"use client"

import { useState } from "react"
import { ImageIcon } from "lucide-react"

import { artifactVisualKind } from "@/components/layout/task-artifact-type"
import type { TaskArtifactInfo } from "@/lib/api"
import { buildArtifactThumbnailUrl } from "@/lib/artifact-image-thumbnail"

export function CompactReplyImageStrip({
  items,
  remaining,
  onSelect,
  onViewAll,
}: {
  items: TaskArtifactInfo[]
  remaining: number
  onSelect: (item: TaskArtifactInfo) => void
  onViewAll: () => void
}) {
  return (
    <div className="flex min-w-max gap-2 px-1">
      {items.map((item) => (
        <button
          key={`${item.id}:${item.lastCheckedAt}`}
          type="button"
          className="flex size-20 items-center justify-center overflow-hidden rounded-md border bg-muted/25 p-1 transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:size-24"
          title={item.displayName}
          aria-label={item.displayName}
          onClick={() => onSelect(item)}
        >
          <ArtifactImageThumbnail item={item} />
        </button>
      ))}
      {remaining > 0 && (
        <button
          type="button"
          className="flex size-20 shrink-0 items-center justify-center rounded-md border bg-muted/25 text-sm font-medium text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring sm:size-24"
          onClick={onViewAll}
        >
          +{remaining}
        </button>
      )}
    </div>
  )
}

function ArtifactImageThumbnail({ item }: { item: TaskArtifactInfo }) {
  const original = item.path
  const thumbnail = buildArtifactThumbnailUrl(original)
  const [source, setSource] = useState(thumbnail)
  const [failed, setFailed] = useState(false)
  if (failed) {
    return <ImageIcon className="size-7 text-muted-foreground" />
  }
  return (
    // Runtime object-storage hosts cannot be declared in Next's static allowlist.
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={source}
      alt={item.displayName}
      loading="lazy"
      decoding="async"
      className="size-full object-contain"
      onError={() => {
        if (source !== original) setSource(original)
        else setFailed(true)
      }}
    />
  )
}

export function isRemoteImageArtifact(item: TaskArtifactInfo): boolean {
  return (
    item.status === "available" &&
    item.kind === "url" &&
    artifactVisualKind(item) === "image"
  )
}
