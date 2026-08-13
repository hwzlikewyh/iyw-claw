"use client"

import { useEffect, useState } from "react"
import { AlertCircle, File, Folder, Loader2, PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { ScrollArea } from "@/components/ui/scroll-area"
import { listTaskArtifacts, type TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

export interface ProjectReferenceSelection {
  path: string
  name: string
  kind: "file" | "dir"
}

export interface ReferenceArtifactsState {
  items: TaskArtifactInfo[]
  loading: boolean
  error: boolean
}

interface StoredArtifactsState extends ReferenceArtifactsState {
  folderId: number | null
}

export function useReferenceArtifacts(
  open: boolean,
  folderId: number | null
): ReferenceArtifactsState {
  const [state, setState] = useState<StoredArtifactsState>({
    folderId: null,
    items: [],
    loading: false,
    error: false,
  })

  useEffect(() => {
    if (!open || folderId == null) return
    let cancelled = false
    listTaskArtifacts({ folderId })
      .then((items) => {
        if (cancelled) return
        setState({
          folderId,
          items: visibleArtifacts(items),
          loading: false,
          error: false,
        })
      })
      .catch((reason) => {
        if (cancelled) return
        console.error("[project-reference] artifact list failed", {
          folderId,
          errorType: reason instanceof Error ? reason.name : typeof reason,
        })
        setState({ folderId, items: [], loading: false, error: true })
      })
    return () => {
      cancelled = true
    }
  }, [folderId, open])

  if (!open || folderId == null) {
    return { items: [], loading: false, error: false }
  }
  if (state.folderId !== folderId) {
    return { items: [], loading: true, error: false }
  }
  return state
}

function visibleArtifacts(items: TaskArtifactInfo[]): TaskArtifactInfo[] {
  return items.filter(
    (item) => item.status === "available" && item.kind !== "url"
  )
}

export function ArtifactReferencePicker({
  items,
  loading,
  error,
  selected,
  onSelect,
}: ReferenceArtifactsState & {
  selected: ProjectReferenceSelection | null
  onSelect: (selection: ProjectReferenceSelection) => void
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  if (loading) return <ArtifactState icon={Loader2} text={t("loading")} spin />
  if (error) return <ArtifactState icon={AlertCircle} text={t("loadError")} />
  if (items.length === 0)
    return <ArtifactState icon={PackageOpen} text={t("emptyArtifacts")} />

  return (
    <ScrollArea className="h-full min-h-0">
      <div className="space-y-1 p-2">
        {items.map((item) => (
          <ArtifactReferenceRow
            key={item.id}
            item={item}
            selected={selected?.path === item.path}
            onSelect={onSelect}
          />
        ))}
      </div>
    </ScrollArea>
  )
}

function ArtifactReferenceRow({
  item,
  selected,
  onSelect,
}: {
  item: TaskArtifactInfo
  selected: boolean
  onSelect: (selection: ProjectReferenceSelection) => void
}) {
  const t = useTranslations("Folder.chat.messageInput.projectReference")
  const kind = item.kind === "directory" ? "dir" : "file"
  const Icon = kind === "dir" ? Folder : File
  return (
    <button
      type="button"
      onClick={() =>
        onSelect({ path: item.path, name: item.displayName, kind })
      }
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left hover:bg-muted/60",
        selected && "bg-muted"
      )}
    >
      <Icon className="size-4 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{item.displayName}</span>
        <span className="block truncate text-xs text-muted-foreground">
          {item.conversationTitle || t("untitledConversation")}
        </span>
      </span>
    </button>
  )
}

function ArtifactState({
  icon: Icon,
  text,
  spin = false,
}: {
  icon: typeof Loader2
  text: string
  spin?: boolean
}) {
  return (
    <div className="flex min-h-48 items-center justify-center gap-2 text-sm text-muted-foreground">
      <Icon className={cn("size-4", spin && "animate-spin")} />
      {text}
    </div>
  )
}
