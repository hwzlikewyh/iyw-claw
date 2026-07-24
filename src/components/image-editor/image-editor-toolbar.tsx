"use client"

import type { ReactNode } from "react"
import {
  ArrowUpRight,
  Check,
  Circle,
  Crop,
  Eraser,
  Eye,
  MessageSquarePlus,
  MoreHorizontal,
  MousePointer2,
  Pencil,
  Redo2,
  Scan,
  Square,
  Trash2,
  Type,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import {
  clampImageZoom,
  IMAGE_ZOOM_MAX,
  IMAGE_ZOOM_MIN,
  IMAGE_ZOOM_STEP,
  type EditorTool,
  type ImageEditorToolbarProps,
} from "./image-editor-model"
import { ImageEditorStyleControls } from "./image-editor-style-controls"

interface ToolbarButtonProps {
  label: string
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}

function ToolbarButton(props: ToolbarButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          disabled={props.disabled}
          aria-label={props.label}
          aria-pressed={props.active}
          onClick={props.onClick}
          className={cn(
            "rounded-md text-white/70 hover:bg-white/10 hover:text-white",
            props.active && "bg-white/14 text-white"
          )}
        >
          {props.children}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={6}>
        {props.label}
      </TooltipContent>
    </Tooltip>
  )
}

function Separator() {
  return <span className="mx-0.5 h-6 w-px shrink-0 bg-white/10" aria-hidden />
}

function ToolControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  const tools: { tool: EditorTool; label: string; icon: ReactNode }[] = [
    { tool: "select", label: t("imageEditorSelect"), icon: <MousePointer2 /> },
    { tool: "crop", label: t("imageEditorCrop"), icon: <Crop /> },
    { tool: "rectangle", label: t("imageEditorRectangle"), icon: <Square /> },
    { tool: "ellipse", label: t("imageEditorEllipse"), icon: <Circle /> },
    { tool: "arrow", label: t("imageEditorArrow"), icon: <ArrowUpRight /> },
    { tool: "freehand", label: t("imageEditorDraw"), icon: <Pencil /> },
    { tool: "text", label: t("imageEditorText"), icon: <Type /> },
  ]
  return (
    <>
      {tools.map((item) => (
        <ToolbarButton
          key={item.tool}
          label={item.label}
          active={editor.tool === item.tool}
          disabled={!editor.ready || editor.busy}
          onClick={() => editor.onToolChange(item.tool)}
        >
          {item.icon}
        </ToolbarButton>
      ))}
    </>
  )
}

function HistoryControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <>
      <ToolbarButton
        label={t("imageEditorUndo")}
        disabled={!editor.canUndo || editor.busy}
        onClick={editor.onUndo}
      >
        <Undo2 />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageEditorRedo")}
        disabled={!editor.canRedo || editor.busy}
        onClick={editor.onRedo}
      >
        <Redo2 />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageEditorDelete")}
        disabled={!editor.selectedId || editor.busy}
        onClick={editor.onDelete}
      >
        <Trash2 />
      </ToolbarButton>
    </>
  )
}

function ClearMenu({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          disabled={!editor.hasEdits || editor.busy}
          aria-label={t("imageEditorMore")}
          title={t("imageEditorMore")}
          className="rounded-md text-white/70 hover:bg-white/10 hover:text-white"
        >
          <MoreHorizontal />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        side="top"
        align="end"
        sideOffset={10}
        className="w-auto rounded-md border border-white/10 bg-zinc-900 p-1.5 text-white"
      >
        <Button
          type="button"
          variant="destructive"
          size="sm"
          onClick={editor.onClear}
          className="w-full justify-start rounded-[5px]"
        >
          <Eraser />
          {t("imageEditorClear")}
        </Button>
      </PopoverContent>
    </Popover>
  )
}

function ZoomControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  const change = (delta: number) =>
    editor.onZoomChange(clampImageZoom(editor.zoom + delta))
  return (
    <>
      <ToolbarButton
        label={t("imageEditorZoomOut")}
        disabled={!editor.ready || editor.zoom <= IMAGE_ZOOM_MIN}
        onClick={() => change(-IMAGE_ZOOM_STEP)}
      >
        <ZoomOut />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageViewerFit")}
        disabled={!editor.ready || editor.zoom === 1}
        onClick={() => editor.onZoomChange(1)}
      >
        <Scan />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageEditorZoomIn")}
        disabled={!editor.ready || editor.zoom >= IMAGE_ZOOM_MAX}
        onClick={() => change(IMAGE_ZOOM_STEP)}
      >
        <ZoomIn />
      </ToolbarButton>
    </>
  )
}

function ActionControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  const disabled = !editor.ready || editor.busy
  return (
    <>
      {editor.canApply ? (
        <Button
          type="button"
          size="sm"
          disabled={disabled}
          aria-label={t("imageEditorApply")}
          onClick={editor.onApply}
          className="shrink-0 rounded-md"
        >
          <Check />
          <span className="max-[479px]:hidden">{t("imageEditorApply")}</span>
        </Button>
      ) : null}
      {editor.canSendToChat ? (
        <Button
          type="button"
          size="sm"
          disabled={disabled}
          aria-label={t("imageEditorSendToChat")}
          onClick={editor.onSendToChat}
          className="shrink-0 rounded-md"
        >
          <MessageSquarePlus />
          <span className="max-[479px]:hidden">
            {t("imageEditorSendToChat")}
          </span>
        </Button>
      ) : null}
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={editor.busy}
        aria-label={t("imageEditorDone")}
        onClick={editor.onDone}
        className="shrink-0 rounded-md bg-white/12 text-white hover:bg-white/18"
      >
        <Eye />
        <span className="max-[479px]:hidden">{t("imageEditorDone")}</span>
      </Button>
    </>
  )
}

export function ImageEditorToolbar(editor: ImageEditorToolbarProps) {
  return (
    <TooltipProvider>
      <div className="flex h-11 min-w-0 max-w-full overflow-hidden rounded-md border border-white/10 bg-zinc-900/95 shadow-xl backdrop-blur">
        <div className="min-w-0 flex-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          <div className="flex h-full w-max items-center gap-1 px-1.5">
            <ToolControls editor={editor} />
            <Separator />
            <ImageEditorStyleControls editor={editor} />
            <HistoryControls editor={editor} />
            <ClearMenu editor={editor} />
            <Separator />
            <ZoomControls editor={editor} />
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1 border-l border-white/10 px-1.5">
          <ActionControls editor={editor} />
        </div>
      </div>
    </TooltipProvider>
  )
}
