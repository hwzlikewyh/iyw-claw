"use client"

import { useCallback, useMemo } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { useActiveFolder } from "@/contexts/active-folder-context"
import { useWorkspaceActions } from "@/contexts/workspace-context"
import { usePlatform } from "@/hooks/use-platform"
import type { TaskArtifactInfo } from "@/lib/api"
import { findOwningFolder } from "@/lib/file-open-target"
import {
  copyFileToClipboard,
  isLocalDesktop,
  openPath,
  openPathWithPicker,
  revealItemInDir,
} from "@/lib/platform"
import { copyTextFromMenu } from "@/lib/utils"

export interface TaskArtifactTarget {
  rootPath: string
  ioPath: string
}

interface UseTaskArtifactActionsOptions {
  artifact: TaskArtifactInfo
  onPreview: (artifact: TaskArtifactInfo) => void
  onOpenWorkspace?: () => void
}

export interface TaskArtifactActions {
  target: TaskArtifactTarget | null
  canOpenWorkspace: boolean
  canUseSystem: boolean
  canChooseApplication: boolean
  canCopyFile: boolean
  preview: () => void
  openWorkspace: () => Promise<void>
  copyFile: () => Promise<void>
  openDefault: () => Promise<void>
  openWith: () => Promise<void>
  reveal: () => Promise<void>
  copyPath: () => Promise<void>
}

type ArtifactAction =
  | "copyFile"
  | "copyPath"
  | "open"
  | "openWith"
  | "reveal"
  | "workspace"
interface ArtifactActionRequest {
  action: ArtifactAction
  task: () => Promise<void>
  failure: string
  success?: string
}
type ArtifactActionRunner = (request: ArtifactActionRequest) => Promise<void>

function useArtifactTarget(
  artifact: TaskArtifactInfo
): TaskArtifactTarget | null {
  const { activeFolder } = useActiveFolder()
  return useMemo(() => {
    if (!activeFolder || artifact.folderId !== activeFolder.id) return null
    const owning = findOwningFolder(artifact.path, [activeFolder])
    return owning ? { rootPath: owning.rootPath, ioPath: owning.relPath } : null
  }, [activeFolder, artifact])
}

function useArtifactActionRunner(
  artifact: TaskArtifactInfo
): ArtifactActionRunner {
  return useCallback(
    async ({ action, task, failure, success }) => {
      try {
        await task()
        if (success) toast.success(success)
      } catch (error) {
        console.error("[task-artifacts] action failed", {
          action,
          artifactId: artifact.id,
          status: artifact.status,
          environment: isLocalDesktop() ? "local-desktop" : "remote-or-web",
          ...artifactActionErrorContext(error),
        })
        toast.error(failure)
      }
    },
    [artifact.id, artifact.status]
  )
}

function artifactActionErrorContext(error: unknown): {
  errorType: string
  errorCode?: string
} {
  if (error instanceof Error) return { errorType: error.name }
  if (!error || typeof error !== "object") {
    return { errorType: typeof error }
  }
  const code = "code" in error ? error.code : undefined
  return {
    errorType: error.constructor?.name ?? "Object",
    errorCode: typeof code === "string" ? code : undefined,
  }
}

export function useTaskArtifactActions({
  artifact,
  onPreview,
  onOpenWorkspace,
}: UseTaskArtifactActionsOptions): TaskArtifactActions {
  const t = useTranslations("Folder.taskArtifacts")
  const { openFilePreview } = useWorkspaceActions()
  const { isWindows } = usePlatform()
  const target = useArtifactTarget(artifact)
  const run = useArtifactActionRunner(artifact)
  const canUseSystem = isLocalDesktop() && artifact.status === "available"
  const canOpenWorkspace = target !== null && artifact.status === "available"
  const canCopyFile = canUseSystem && isWindows
  return createTaskArtifactActions({
    artifact,
    target,
    run,
    canUseSystem,
    canOpenWorkspace,
    canCopyFile,
    isWindows,
    onPreview,
    onOpenWorkspace,
    openFilePreview,
    copyFailed: t("copyFailed"),
    copyFileFailed: t("copyFileFailed"),
    fileCopied: t("fileCopied"),
    pathCopied: t("pathCopied"),
    openFailed: t("openFailed"),
    openWithFailed: t("openWithFailed"),
    revealFailed: t("revealFailed"),
    openWorkspaceFailed: t("openWorkspaceFailed"),
  })
}

interface ArtifactActionFactoryOptions extends UseTaskArtifactActionsOptions {
  target: TaskArtifactTarget | null
  run: ArtifactActionRunner
  canUseSystem: boolean
  canOpenWorkspace: boolean
  canCopyFile: boolean
  isWindows: boolean
  openFilePreview: (path: string) => Promise<void>
  copyFailed: string
  copyFileFailed: string
  fileCopied: string
  pathCopied: string
  openFailed: string
  openWithFailed: string
  revealFailed: string
  openWorkspaceFailed: string
}

function createTaskArtifactActions(
  options: ArtifactActionFactoryOptions
): TaskArtifactActions {
  const { artifact, run, target } = options
  return {
    target,
    canUseSystem: options.canUseSystem,
    canOpenWorkspace: options.canOpenWorkspace,
    canChooseApplication: options.canUseSystem && options.isWindows,
    canCopyFile: options.canCopyFile,
    preview: () => options.onPreview(artifact),
    copyFile: () =>
      run({
        action: "copyFile",
        task: () => copyFileToClipboard(artifact.path),
        failure: options.copyFileFailed,
        success: options.fileCopied,
      }),
    copyPath: () =>
      copyArtifactPath({
        run,
        path: artifact.path,
        failure: options.copyFailed,
        success: options.pathCopied,
      }),
    openDefault: () =>
      run({
        action: "open",
        task: () => openPath(artifact.path),
        failure: options.openFailed,
      }),
    openWith: () =>
      run({
        action: "openWith",
        task: () => openPathWithPicker(artifact.path),
        failure: options.openWithFailed,
      }),
    reveal: () =>
      run({
        action: "reveal",
        task: () => revealItemInDir(artifact.path),
        failure: options.revealFailed,
      }),
    openWorkspace: () =>
      openArtifactInWorkspace({
        run,
        path: artifact.path,
        target,
        onOpenWorkspace: options.onOpenWorkspace,
        openFilePreview: options.openFilePreview,
        failure: options.openWorkspaceFailed,
      }),
  }
}

function copyArtifactPath({
  run,
  path,
  failure,
  success,
}: {
  run: ArtifactActionRunner
  path: string
  failure: string
  success: string
}): Promise<void> {
  return run({
    action: "copyPath",
    task: async () => {
      if (!(await copyTextFromMenu(path))) throw new Error("copy")
    },
    failure,
    success,
  })
}

function openArtifactInWorkspace({
  run,
  path,
  target,
  onOpenWorkspace,
  openFilePreview,
  failure,
}: {
  run: ArtifactActionRunner
  path: string
  target: TaskArtifactTarget | null
  onOpenWorkspace?: () => void
  openFilePreview: (path: string) => Promise<void>
  failure: string
}): Promise<void> {
  return run({
    action: "workspace",
    task: async () => {
      if (!target) throw new Error("workspace target")
      onOpenWorkspace?.()
      await openFilePreview(path)
    },
    failure,
  })
}
