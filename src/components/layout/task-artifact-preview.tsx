"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"

import {
  useTaskArtifactActions,
  type TaskArtifactTarget,
} from "@/components/layout/task-artifact-actions"
import { TaskArtifactDirectoryPreview } from "@/components/layout/task-artifact-directory-preview"
import { EmptyTaskArtifactPreview } from "@/components/layout/task-artifact-preview-empty"
import { TaskArtifactPreviewHeader } from "@/components/layout/task-artifact-preview-header"
import {
  resolveArtifactPreview,
  startArtifactPreviewLoad,
  type ArtifactPreviewSource,
  type LoadedArtifactPreview,
} from "@/components/layout/task-artifact-preview-loader"
import {
  WorkspaceFilePreview,
  type PreviewState,
} from "@/components/message/workspace-file-preview"
import type { TaskArtifactInfo } from "@/lib/api"
import { cn } from "@/lib/utils"

interface TaskArtifactPreviewProps {
  artifact: TaskArtifactInfo | null
  className?: string
  onBack?: () => void
  onOpenWorkspace?: () => void
  onPreview?: (artifact: TaskArtifactInfo) => void
}

type ArtifactPreviewProps = Omit<TaskArtifactPreviewProps, "artifact"> & {
  artifact: TaskArtifactInfo
}

export function TaskArtifactPreview({
  artifact,
  className,
  onBack,
  onOpenWorkspace,
  onPreview,
}: TaskArtifactPreviewProps) {
  if (!artifact) return <EmptyTaskArtifactPreview className={className} />
  return (
    <ArtifactPreview
      artifact={artifact}
      className={className}
      onBack={onBack}
      onOpenWorkspace={onOpenWorkspace}
      onPreview={onPreview}
    />
  )
}

function ArtifactPreview({
  artifact,
  className,
  onBack,
  onOpenWorkspace,
  onPreview = () => undefined,
}: ArtifactPreviewProps) {
  const actions = useTaskArtifactActions({
    artifact,
    onPreview,
    onOpenWorkspace,
  })
  return (
    <section
      aria-label={artifact.displayName}
      className={cn(
        "grid min-h-0 grid-rows-[auto_minmax(0,1fr)] bg-background",
        className
      )}
    >
      <TaskArtifactPreviewHeader
        artifact={artifact}
        actions={actions}
        onBack={onBack}
      />
      <div className="min-h-0">
        <ArtifactPreviewBody artifact={artifact} target={actions.target} />
      </div>
    </section>
  )
}

function ArtifactPreviewBody({
  artifact,
  target,
}: {
  artifact: TaskArtifactInfo
  target: TaskArtifactTarget | null
}) {
  if (artifact.kind === "directory") {
    return <TaskArtifactDirectoryPreview artifact={artifact} />
  }
  return <TaskArtifactFilePreview artifact={artifact} target={target} />
}

function TaskArtifactFilePreview({
  artifact,
  target,
}: {
  artifact: TaskArtifactInfo
  target: TaskArtifactTarget | null
}) {
  const preview = useArtifactPreviewState(artifact, target)
  return (
    <WorkspaceFilePreview state={preview} rootPath={target?.rootPath ?? ""} />
  )
}

function useArtifactPreviewState(
  artifact: TaskArtifactInfo,
  target: TaskArtifactTarget | null
): PreviewState {
  const t = useTranslations("Folder.taskArtifacts")
  const loaded = useLoadedArtifactPreview(artifact, target, t("previewFailed"))
  return resolveArtifactPreview(artifact, target, loaded, {
    unavailable: t("artifactUnavailable"),
    failed: t("previewFailed"),
  })
}

function useLoadedArtifactPreview(
  artifact: TaskArtifactInfo,
  target: TaskArtifactTarget | null,
  failureMessage: string
): LoadedArtifactPreview | null {
  const [loaded, setLoaded] = useState<LoadedArtifactPreview | null>(null)
  const key = `${artifact.id}:${artifact.lastCheckedAt}`
  const { kind, path, status } = artifact
  useEffect(() => {
    const source: ArtifactPreviewSource = { key, kind, path, status }
    return startArtifactPreviewLoad(source, target, failureMessage, setLoaded)
  }, [failureMessage, key, kind, path, status, target])
  return loaded
}
