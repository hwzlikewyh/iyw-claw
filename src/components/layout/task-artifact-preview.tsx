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
  WorkspaceFilePreview,
  type PreviewState,
} from "@/components/message/workspace-file-preview"
import { loadWorkspacePreview } from "@/components/message/workspace-file-preview-loader"
import type { TaskArtifactInfo } from "@/lib/api"
import { isOfficePreviewable } from "@/lib/language-detect"
import { cn } from "@/lib/utils"

interface LoadedPreview {
  key: string
  state: PreviewState
}

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
  const previewKey =
    artifact.status === "available" && target
      ? `${artifact.id}:${artifact.lastCheckedAt}`
      : null
  const loaded = useLoadedArtifactPreview(
    previewKey,
    target,
    t("previewFailed")
  )
  return resolveArtifactPreview({
    artifact,
    target,
    previewKey,
    loaded,
    unavailable: t("artifactUnavailable"),
    previewFailed: t("previewFailed"),
  })
}

function useLoadedArtifactPreview(
  previewKey: string | null,
  target: TaskArtifactTarget | null,
  failureMessage: string
): LoadedPreview | null {
  const [loaded, setLoaded] = useState<LoadedPreview | null>(null)
  useEffect(() => {
    if (!previewKey || !target || isOfficePreviewable(target.ioPath)) return
    let active = true
    void loadWorkspacePreview(target.rootPath, target.ioPath)
      .then((next) => active && setLoaded({ key: previewKey, state: next }))
      .catch(
        () =>
          active &&
          setLoaded({
            key: previewKey,
            state: {
              status: "error",
              path: target.ioPath,
              message: failureMessage,
            },
          })
      )
    return () => {
      active = false
    }
  }, [failureMessage, previewKey, target])
  return loaded
}

function resolveArtifactPreview({
  artifact,
  target,
  previewKey,
  loaded,
  unavailable,
  previewFailed,
}: {
  artifact: TaskArtifactInfo
  target: TaskArtifactTarget | null
  previewKey: string | null
  loaded: LoadedPreview | null
  unavailable: string
  previewFailed: string
}): PreviewState {
  if (artifact.status !== "available") {
    return {
      status: "error",
      path: artifact.path,
      message: unavailable,
    }
  }
  if (!target) {
    return {
      status: "error",
      path: artifact.path,
      message: previewFailed,
    }
  }
  if (isOfficePreviewable(target.ioPath)) {
    return { status: "office", path: target.ioPath }
  }
  if (previewKey && loaded?.key === previewKey) return loaded.state
  return { status: "loading", path: target.ioPath }
}
