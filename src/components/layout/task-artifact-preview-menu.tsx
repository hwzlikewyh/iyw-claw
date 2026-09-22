"use client"

import { useRef, useState, type ReactNode, type RefObject } from "react"
import { Copy, TextSelect } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import type { TaskArtifactActions } from "./task-artifact-actions"
import {
  TASK_ARTIFACT_MENU_CONTENT_CLASS,
  TaskArtifactContextMenuItems,
} from "./task-artifact-menu"
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu"
import { copyTextFromMenu } from "@/lib/utils"

function selectedPreviewText(root: HTMLElement | null): string {
  const selection = window.getSelection()
  if (!root || !selection?.rangeCount) return ""
  const range = selection.getRangeAt(0)
  if (
    !root.contains(range.startContainer) ||
    !root.contains(range.endContainer)
  )
    return ""
  return selection.toString()
}

function previewTextContent(root: HTMLElement | null): HTMLElement | null {
  return root?.querySelector("[data-artifact-preview-text]") ?? null
}

function usePreviewTextMenu(root: RefObject<HTMLElement | null>) {
  const [text, setText] = useState("")
  const [canSelectAll, setCanSelectAll] = useState(false)
  const t = useTranslations("Folder.taskArtifacts")
  const selectAll = () => {
    requestAnimationFrame(() => {
      const content = previewTextContent(root.current)
      if (!content) return
      const range = document.createRange()
      range.selectNodeContents(content)
      const selection = window.getSelection()
      selection?.removeAllRanges()
      selection?.addRange(range)
    })
  }
  const copy = async () => {
    if (!text) return
    if (await copyTextFromMenu(text)) toast.success(t("textCopied"))
    else toast.error(t("copyTextFailed"))
  }
  const onOpenChange = (open: boolean) => {
    if (!open) return
    setText(selectedPreviewText(root.current))
    setCanSelectAll(Boolean(previewTextContent(root.current)?.textContent))
  }
  return { text, canSelectAll, selectAll, copy, onOpenChange }
}

export function TaskArtifactPreviewMenu({
  actions,
  children,
}: {
  actions: TaskArtifactActions
  children: ReactNode
}) {
  const root = useRef<HTMLSpanElement>(null)
  const textMenu = usePreviewTextMenu(root)
  return (
    <ContextMenu onOpenChange={textMenu.onOpenChange}>
      <ContextMenuTrigger
        asChild
        ref={root}
        onPointerDown={(event) => {
          if (event.button === 2 && selectedPreviewText(root.current))
            event.preventDefault()
        }}
      >
        {children}
      </ContextMenuTrigger>
      <ContextMenuContent className={TASK_ARTIFACT_MENU_CONTENT_CLASS}>
        <PreviewTextMenuItems {...textMenu} />
        <ContextMenuSeparator />
        <TaskArtifactContextMenuItems
          actions={actions}
          includePreview={false}
        />
      </ContextMenuContent>
    </ContextMenu>
  )
}

function PreviewTextMenuItems({
  text,
  canSelectAll,
  copy,
  selectAll,
}: ReturnType<typeof usePreviewTextMenu>) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <>
      <ContextMenuItem disabled={!text} onSelect={() => void copy()}>
        <Copy />
        {t("copySelectedText")}
      </ContextMenuItem>
      <ContextMenuItem disabled={!canSelectAll} onSelect={selectAll}>
        <TextSelect />
        {t("selectAllText")}
      </ContextMenuItem>
    </>
  )
}
