"use client"

import { useCallback, useMemo } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { useWorkspaceActions } from "@/contexts/workspace-context"
import { usePlatform } from "@/hooks/use-platform"
import type { TaskArtifactInfo } from "@/lib/api"
import { splitAbsPath } from "@/lib/file-open-target"
import {
  copyArtifactToClipboard,
  isLocalDesktop,
  openUrl,
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
  kind: TaskArtifactInfo["kind"]
  target: TaskArtifactTarget | null
  canOpenWorkspace: boolean
  canUseSystem: boolean
  canOpenExternal: boolean
  canChooseApplication: boolean
  canCopyItem: boolean
  preview: () => void
  openWorkspace: () => Promise<void>
  copyItem: () => Promise<void>
  openDefault: () => Promise<void>
  openWith: () => Promise<void>
  reveal: () => Promise<void>
  copyPath: () => Promise<void>
}

type ArtifactAction =
  | "copyItem"
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
type ArtifactSystemActions = Pick<
  TaskArtifactActions,
  "openDefault" | "openWith" | "reveal"
>

function useArtifactTarget(
  artifact: TaskArtifactInfo
): TaskArtifactTarget | null {
  return useMemo(() => {
    if (artifact.kind === "directory" || artifact.kind === "url") return null
    return splitAbsPath(artifact.path)
  }, [artifact.kind, artifact.path])
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
          artifactKind: artifact.kind,
          status: artifact.status,
          environment: isLocalDesktop() ? "local-desktop" : "remote-or-web",
          ...artifactActionErrorContext(error),
        })
        toast.error(failure)
      }
    },
    [artifact.id, artifact.kind, artifact.status]
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
  const available = artifact.status === "available"
  const directory = artifact.kind === "directory"
  const canUseSystem = artifact.kind !== "url" && isLocalDesktop() && available
  const canOpenExternal = artifact.kind === "url" && available
  const canOpenWorkspace = !directory && target !== null && available
  const canCopyItem = canUseSystem && isWindows
  return createTaskArtifactActions({
    artifact,
    target,
    run,
    canUseSystem,
    canOpenExternal,
    canOpenWorkspace,
    canCopyItem,
    isWindows,
    onPreview,
    onOpenWorkspace,
    openFilePreview,
    copyFailed: artifact.kind === "url" ? t("copyLinkFailed") : t("copyFailed"),
    copyItemFailed: directory ? t("copyFolderFailed") : t("copyFileFailed"),
    itemCopied: directory ? t("folderCopied") : t("fileCopied"),
    pathCopied: artifact.kind === "url" ? t("linkCopied") : t("pathCopied"),
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
  canOpenExternal: boolean
  canOpenWorkspace: boolean
  canCopyItem: boolean
  isWindows: boolean
  openFilePreview: (path: string) => Promise<void>
  copyFailed: string
  copyItemFailed: string
  itemCopied: string
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
    kind: artifact.kind,
    target,
    canUseSystem: options.canUseSystem,
    canOpenExternal: options.canOpenExternal,
    canOpenWorkspace: options.canOpenWorkspace,
    canChooseApplication:
      options.canUseSystem && options.isWindows && artifact.kind === "file",
    canCopyItem: options.canCopyItem,
    preview: () => options.onPreview(artifact),
    copyItem: () =>
      run({
        action: "copyItem",
        task: () => copyArtifactToClipboard(artifact.path),
        failure: options.copyItemFailed,
        success: options.itemCopied,
      }),
    copyPath: () =>
      copyArtifactPath({
        run,
        path: artifact.path,
        failure: options.copyFailed,
        success: options.pathCopied,
      }),
    ...createArtifactSystemActions(options),
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

function createArtifactSystemActions(
  options: ArtifactActionFactoryOptions
): ArtifactSystemActions {
  const { artifact, run } = options
  return {
    openDefault: () =>
      run({
        action: "open",
        task: () =>
          artifact.kind === "url"
            ? openUrl(artifact.path)
            : openPath(artifact.path),
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
