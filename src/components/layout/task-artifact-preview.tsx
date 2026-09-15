"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import type { RefObject } from "react"
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
import { useArtifactSystemFullscreen } from "@/components/layout/use-artifact-system-fullscreen"
import type { TaskArtifactInfo } from "@/lib/api"
import { findOwningFolder } from "@/lib/file-open-target"
import { cn } from "@/lib/utils"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { toast } from "sonner"
import { PreviewFullscreen } from "@/components/files/preview-fullscreen"

interface TaskArtifactPreviewProps {
  artifact: TaskArtifactInfo | null
  className?: string
  onBack?: () => void
  onOpenWorkspace?: () => void
  onPreview?: (artifact: TaskArtifactInfo) => void
}

type ArtifactPreviewProps = Omit<TaskArtifactPreviewProps, "artifact"> & {
  artifact: TaskArtifactInfo
  isAppFullscreen?: boolean
  isSystemFullscreen?: boolean
  fullscreenTargetRef?: RefObject<HTMLElement | null>
  onToggleAppFullscreen?: () => void
  onToggleSystemFullscreen?: () => Promise<void>
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
    <ArtifactPreviewWithFullscreen
      artifact={artifact}
      className={className}
      onBack={onBack}
      onOpenWorkspace={onOpenWorkspace}
      onPreview={onPreview}
    />
  )
}

function ArtifactPreviewWithFullscreen(props: ArtifactPreviewProps) {
  const [appFullscreen, setAppFullscreen] = useState(false)
  const [systemFullscreen, setSystemFullscreen] = useState(false)
  const fullscreenTargetRef = useRef<HTMLElement>(null)
  const requestSystemFullscreen = useArtifactSystemFullscreen({
    enabled: appFullscreen,
    targetRef: fullscreenTargetRef,
    onChange: setSystemFullscreen,
  })
  const t = useTranslations("Folder.taskArtifacts")
  const toggleSystemFullscreen = useCallback(async () => {
    try {
      await requestSystemFullscreen()
    } catch {
      toast.error(t("fullscreenFailed"))
    }
  }, [requestSystemFullscreen, t])

  const closeAppFullscreen = useCallback(() => {
    setAppFullscreen(false)
  }, [])

  return (
    <PreviewFullscreen
      open={appFullscreen}
      onClose={closeAppFullscreen}
      title={props.artifact.displayName}
      className={props.className}
    >
      <ArtifactPreview
        {...props}
        className="h-full"
        isAppFullscreen={appFullscreen}
        isSystemFullscreen={systemFullscreen}
        fullscreenTargetRef={fullscreenTargetRef}
        onToggleAppFullscreen={() => setAppFullscreen((value) => !value)}
        onToggleSystemFullscreen={toggleSystemFullscreen}
      />
    </PreviewFullscreen>
  )
}

function ArtifactPreview({
  artifact,
  className,
  onBack,
  onOpenWorkspace,
  onPreview = () => undefined,
  isAppFullscreen = false,
  isSystemFullscreen = false,
  fullscreenTargetRef,
  onToggleAppFullscreen,
  onToggleSystemFullscreen,
}: ArtifactPreviewProps) {
  const actions = useTaskArtifactActions({
    artifact,
    onPreview,
    onOpenWorkspace,
  })
  return (
    <section
      aria-label={artifact.displayName}
      ref={fullscreenTargetRef}
      className={cn(
        "grid min-h-0 grid-rows-[auto_minmax(0,1fr)] bg-background",
        className
      )}
    >
      <TaskArtifactPreviewHeader
        artifact={artifact}
        actions={actions}
        onBack={onBack}
        isAppFullscreen={isAppFullscreen}
        isSystemFullscreen={isSystemFullscreen}
        onToggleAppFullscreen={onToggleAppFullscreen}
        onToggleSystemFullscreen={onToggleSystemFullscreen}
      />
      <div className="min-h-0">
        <ArtifactPreviewBody
          artifact={artifact}
          target={actions.target}
          onOpenWorkspace={onOpenWorkspace}
        />
      </div>
    </section>
  )
}

function ArtifactPreviewBody({
  artifact,
  target,
  onOpenWorkspace,
}: {
  artifact: TaskArtifactInfo
  target: TaskArtifactTarget | null
  onOpenWorkspace?: () => void
}) {
  if (artifact.kind === "directory") {
    return (
      <TaskArtifactDirectoryPreview
        artifact={artifact}
        onOpenWorkspace={onOpenWorkspace}
      />
    )
  }
  return (
    <TaskArtifactFilePreview
      artifact={artifact}
      target={target}
      onOpenWorkspace={onOpenWorkspace}
    />
  )
}

function TaskArtifactFilePreview({
  artifact,
  target,
  onOpenWorkspace,
}: {
  artifact: TaskArtifactInfo
  target: TaskArtifactTarget | null
  onOpenWorkspace?: () => void
}) {
  const preview = useArtifactPreviewState(artifact, target)
  const folders = useAppWorkspaceStore((store) => store.folders)
  const htmlRootPath = findOwningFolder(artifact.path, folders)?.rootPath
  return (
    <WorkspaceFilePreview
      state={preview}
      rootPath={target?.rootPath ?? ""}
      htmlRootPath={htmlRootPath}
      onOpenWorkspace={onOpenWorkspace}
    />
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
