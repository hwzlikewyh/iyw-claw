"use client"

import { useTranslations } from "next-intl"

import { WorkspaceDirectoryBrowser } from "@/components/message/workspace-directory-browser"
import { WorkspaceFilePreview } from "@/components/message/workspace-file-preview"
import type { TaskArtifactInfo } from "@/lib/api"

export function TaskArtifactDirectoryPreview({
  artifact,
  onOpenWorkspace,
}: {
  artifact: TaskArtifactInfo
  onOpenWorkspace?: () => void
}) {
  const t = useTranslations("Folder.taskArtifacts")
  if (artifact.status !== "available") {
    return (
      <WorkspaceFilePreview
        rootPath=""
        state={{
          status: "error",
          path: artifact.displayName,
          message: t("artifactUnavailable"),
        }}
      />
    )
  }
  return (
    <WorkspaceDirectoryBrowser
      key={`${artifact.id}:${artifact.path}`}
      rootPath={artifact.path}
      className="h-full"
      renderMarkdown
      renderHtml
      renderPdf
      onOpenWorkspace={onOpenWorkspace}
    />
  )
}
