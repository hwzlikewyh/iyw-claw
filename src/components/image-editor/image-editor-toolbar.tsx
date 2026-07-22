"use client"

import type { ReactNode } from "react"
import {
  ArrowUpRight,
  Check,
  Circle,
  Crop,
  Download,
  Eraser,
  MousePointer2,
  Pencil,
  Redo2,
  Scan,
  Square,
  Trash2,
  Type,
  Undo2,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import type { EditorTool, ImageEditorToolbarProps } from "./image-editor-model"

interface ToolbarButtonProps {
  label: string
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: ReactNode
}

const COLORS = ["#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#f8fafc"]
const STROKE_WIDTHS = [2, 4, 8]
const MIN_ZOOM = 0.5
const MAX_ZOOM = 3
const ZOOM_STEP = 0.25

function ToolbarButton(props: ToolbarButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant={props.active ? "secondary" : "ghost"}
          size="icon-sm"
          disabled={props.disabled}
          aria-label={props.label}
          aria-pressed={props.active}
          onClick={props.onClick}
          className="rounded-md"
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
  return <span className="mx-0.5 h-6 w-px shrink-0 bg-border" aria-hidden />
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
      {editor.tool === "text" && (
        <Input
          value={editor.text}
          onChange={(event) => editor.onTextChange(event.target.value)}
          placeholder={t("imageEditorTextPlaceholder")}
          aria-label={t("imageEditorTextPlaceholder")}
          className="h-8 w-36 rounded-md bg-background"
        />
      )}
    </>
  )
}

function ColorControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div
      className="flex h-8 items-center gap-1 px-1"
      aria-label={t("imageEditorColor")}
    >
      {COLORS.map((color) => (
        <button
          key={color}
          type="button"
          onClick={() => editor.onColorChange(color)}
          aria-label={t("imageEditorChooseColor", { color })}
          aria-pressed={editor.color === color}
          className={cn(
            "size-5 rounded-full border border-white/40 shadow-sm outline-none transition-transform hover:scale-110 focus-visible:ring-2 focus-visible:ring-ring",
            editor.color === color &&
              "ring-2 ring-ring ring-offset-1 ring-offset-background"
          )}
          style={{ backgroundColor: color }}
        />
      ))}
    </div>
  )
}

function StrokeControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div
      className="flex h-8 items-center gap-0.5"
      aria-label={t("imageEditorStrokeWidth")}
    >
      {STROKE_WIDTHS.map((width) => (
        <ToolbarButton
          key={width}
          label={t("imageEditorStrokeWidthValue", { width })}
          active={editor.strokeWidth === width}
          onClick={() => editor.onStrokeWidthChange(width)}
        >
          <span
            className="rounded-full bg-current"
            style={{ width, height: width }}
          />
        </ToolbarButton>
      ))}
    </div>
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
      <ToolbarButton
        label={t("imageEditorClear")}
        disabled={!editor.ready || editor.busy}
        onClick={editor.onClear}
      >
        <Eraser />
      </ToolbarButton>
    </>
  )
}

function ZoomControls({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  const change = (delta: number) =>
    editor.onZoomChange(
      Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, editor.zoom + delta))
    )
  return (
    <>
      <ToolbarButton
        label={t("imageEditorZoomOut")}
        disabled={!editor.ready || editor.zoom <= MIN_ZOOM}
        onClick={() => change(-ZOOM_STEP)}
      >
        <ZoomOut />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageEditorResetZoom")}
        disabled={!editor.ready || editor.zoom === 1}
        onClick={() => editor.onZoomChange(1)}
      >
        <Scan />
      </ToolbarButton>
      <ToolbarButton
        label={t("imageEditorZoomIn")}
        disabled={!editor.ready || editor.zoom >= MAX_ZOOM}
        onClick={() => change(ZOOM_STEP)}
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
      {editor.canExport && (
        <ToolbarButton
          label={t("imageEditorExport")}
          disabled={disabled}
          onClick={editor.onExport}
        >
          <Download />
        </ToolbarButton>
      )}
      {editor.canApply && (
        <ToolbarButton
          label={t("imageEditorApply")}
          disabled={disabled}
          onClick={editor.onApply}
        >
          <Check />
        </ToolbarButton>
      )}
      <ToolbarButton
        label={t("imageEditorClose")}
        disabled={editor.busy}
        onClick={editor.onClose}
      >
        <X />
      </ToolbarButton>
    </>
  )
}

export function ImageEditorToolbar(editor: ImageEditorToolbarProps) {
  return (
    <TooltipProvider>
      <div className="flex max-w-full flex-wrap items-center justify-center gap-1 rounded-md border border-border/70 bg-background/95 p-1.5 shadow-xl backdrop-blur">
        <ToolControls editor={editor} />
        <Separator />
        <ColorControls editor={editor} />
        <StrokeControls editor={editor} />
        <Separator />
        <HistoryControls editor={editor} />
        <Separator />
        <ZoomControls editor={editor} />
        <Separator />
        <ActionControls editor={editor} />
      </div>
    </TooltipProvider>
  )
}
