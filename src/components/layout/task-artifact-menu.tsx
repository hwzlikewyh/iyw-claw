"use client"

import { Fragment, useMemo, type ReactNode } from "react"
import {
  AppWindow,
  Copy,
  ExternalLink,
  Files,
  FolderSearch,
  PanelsTopLeft,
  Waypoints,
  type LucideIcon,
} from "lucide-react"
import { useTranslations } from "next-intl"

import type { TaskArtifactActions } from "@/components/layout/task-artifact-actions"
import {
  ContextMenuItem,
  ContextMenuSeparator,
} from "@/components/ui/context-menu"
import {
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/components/ui/dropdown-menu"

interface ArtifactMenuEntry {
  id: string
  section: "preview" | "clipboard" | "system"
  label: string
  icon: LucideIcon
  onSelect: () => void
}

interface ArtifactMenuLabels {
  view: string
  openWorkspace: string
  copyItem: string
  openDefault: string
  openWith: string
  reveal: string
  copyPath: string
}

function previewMenuEntries(
  actions: TaskArtifactActions,
  labels: ArtifactMenuLabels
): ArtifactMenuEntry[] {
  const entries: ArtifactMenuEntry[] = [
    {
      id: "preview",
      section: "preview",
      label: labels.view,
      icon: PanelsTopLeft,
      onSelect: actions.preview,
    },
  ]
  if (actions.canOpenWorkspace) {
    entries.push({
      id: "workspace",
      section: "preview",
      label: labels.openWorkspace,
      icon: Waypoints,
      onSelect: () => void actions.openWorkspace(),
    })
  }
  return entries
}

function systemMenuEntries(
  actions: TaskArtifactActions,
  labels: ArtifactMenuLabels
): ArtifactMenuEntry[] {
  if (!actions.canUseSystem) return []
  const entries: ArtifactMenuEntry[] = [
    {
      id: "open",
      section: "system",
      label: labels.openDefault,
      icon: ExternalLink,
      onSelect: () => void actions.openDefault(),
    },
  ]
  if (actions.canChooseApplication) {
    entries.push({
      id: "openWith",
      section: "system",
      label: labels.openWith,
      icon: AppWindow,
      onSelect: () => void actions.openWith(),
    })
  }
  entries.push({
    id: "reveal",
    section: "system",
    label: labels.reveal,
    icon: FolderSearch,
    onSelect: () => void actions.reveal(),
  })
  return entries
}

function clipboardMenuEntries(
  actions: TaskArtifactActions,
  labels: ArtifactMenuLabels
): ArtifactMenuEntry[] {
  const entries: ArtifactMenuEntry[] = []
  if (actions.canCopyItem) {
    entries.push({
      id: "copyItem",
      section: "clipboard",
      label: labels.copyItem,
      icon: Files,
      onSelect: () => void actions.copyItem(),
    })
  }
  entries.push({
    id: "copyPath",
    section: "clipboard",
    label: labels.copyPath,
    icon: Copy,
    onSelect: () => void actions.copyPath(),
  })
  return entries
}

function useArtifactMenuEntries(
  actions: TaskArtifactActions
): ArtifactMenuEntry[] {
  const t = useTranslations("Folder.taskArtifacts")
  return useMemo(() => {
    const labels: ArtifactMenuLabels = {
      view: t("view"),
      openWorkspace: t("openWorkspace"),
      copyItem: actions.kind === "directory" ? t("copyFolder") : t("copyFile"),
      openDefault: t("openDefault"),
      openWith: t("openWith"),
      reveal: t("reveal"),
      copyPath: t("copyPath"),
    }
    return [
      ...previewMenuEntries(actions, labels),
      ...clipboardMenuEntries(actions, labels),
      ...systemMenuEntries(actions, labels),
    ]
  }, [actions, t])
}

export const TASK_ARTIFACT_MENU_CONTENT_CLASS = "min-w-48 rounded-lg p-1.5"

const TASK_ARTIFACT_MENU_ITEM_CLASS = "gap-2 rounded-md px-2.5 py-1.5"

function ArtifactMenuEntries({
  actions,
  renderItem,
  renderSeparator,
}: {
  actions: TaskArtifactActions
  renderItem: (entry: ArtifactMenuEntry) => ReactNode
  renderSeparator: (id: string) => ReactNode
}) {
  const entries = useArtifactMenuEntries(actions)
  return entries.map((entry, index) => {
    const previous = entries[index - 1]
    return (
      <Fragment key={entry.id}>
        {previous && previous.section !== entry.section
          ? renderSeparator(`separator-${entry.id}`)
          : null}
        {renderItem(entry)}
      </Fragment>
    )
  })
}

export function TaskArtifactContextMenuItems({
  actions,
}: {
  actions: TaskArtifactActions
}) {
  return (
    <ArtifactMenuEntries
      actions={actions}
      renderItem={(entry) => (
        <ContextMenuItem
          className={TASK_ARTIFACT_MENU_ITEM_CLASS}
          onSelect={entry.onSelect}
        >
          <entry.icon />
          {entry.label}
        </ContextMenuItem>
      )}
      renderSeparator={(id) => <ContextMenuSeparator key={id} />}
    />
  )
}

export function TaskArtifactDropdownMenuItems({
  actions,
}: {
  actions: TaskArtifactActions
}) {
  return (
    <ArtifactMenuEntries
      actions={actions}
      renderItem={(entry) => (
        <DropdownMenuItem
          className={TASK_ARTIFACT_MENU_ITEM_CLASS}
          onSelect={entry.onSelect}
        >
          <entry.icon />
          {entry.label}
        </DropdownMenuItem>
      )}
      renderSeparator={(id) => <DropdownMenuSeparator key={id} />}
    />
  )
}
