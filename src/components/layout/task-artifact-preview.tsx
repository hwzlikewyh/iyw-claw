"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import type { RefObject } from "react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog"
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
import { cn } from "@/lib/utils"
import { toast } from "sonner"

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

type AppFullscreenDialogProps = ArtifactPreviewProps & {
  open: boolean
  systemFullscreen: boolean
  fullscreenTargetRef: RefObject<HTMLElement | null>
  onClose: () => void
  onToggleSystemFullscreen: () => Promise<void>
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
    <>
      <ArtifactPreview
        {...props}
        onToggleAppFullscreen={() => setAppFullscreen(true)}
      />
      <AppFullscreenDialog
        {...props}
        open={appFullscreen}
        systemFullscreen={systemFullscreen}
        fullscreenTargetRef={fullscreenTargetRef}
        onClose={closeAppFullscreen}
        onToggleSystemFullscreen={toggleSystemFullscreen}
      />
    </>
  )
}

function AppFullscreenDialog({
  open,
  systemFullscreen,
  fullscreenTargetRef,
  onClose,
  onToggleSystemFullscreen,
  ...previewProps
}: AppFullscreenDialogProps) {
  const { artifact, onBack } = previewProps
  const handleBack = onBack
    ? () => {
        onClose()
        onBack()
      }
    : undefined
  const handleEscape = () => {
    if (systemFullscreen) void onToggleSystemFullscreen()
    else onClose()
  }
  return (
    <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
      <DialogContent
        className="fixed inset-0 h-dvh max-h-none w-dvw max-w-none overflow-hidden rounded-none p-0 sm:max-w-none"
        onEscapeKeyDown={(event) => {
          event.preventDefault()
          handleEscape()
        }}
      >
        <DialogTitle className="sr-only">{artifact.displayName}</DialogTitle>
        <DialogDescription className="sr-only">
          {artifact.displayName}
        </DialogDescription>
        <ArtifactPreview
          {...previewProps}
          className="h-full"
          isAppFullscreen
          isSystemFullscreen={systemFullscreen}
          fullscreenTargetRef={fullscreenTargetRef}
          onBack={handleBack}
          onToggleAppFullscreen={onClose}
          onToggleSystemFullscreen={onToggleSystemFullscreen}
        />
      </DialogContent>
    </Dialog>
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
