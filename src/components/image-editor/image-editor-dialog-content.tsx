"use client"

import dynamic from "next/dynamic"
import { ImageOff, LoaderCircle, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Dialog as DialogPrimitive } from "radix-ui"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { ImageEditorToolbar } from "./image-editor-toolbar"
import type {
  EditorImageResult,
  EditorTool,
  ImageEditorCanvasHandle,
  StageSize,
} from "./image-editor-model"
import { useImageEditor } from "./use-image-editor"

const ImageEditorCanvas = dynamic(
  () =>
    import("./image-editor-canvas").then((module) => module.ImageEditorCanvas),
  { ssr: false }
)

export interface ImageEditorDialogState {
  editor: ReturnType<typeof useImageEditor>
  image: HTMLImageElement | null
  failed: boolean
  stage: StageSize | null
  displayScale: number
  busy: boolean
  ready: boolean
  handleToolChange: (tool: EditorTool) => void
  runAction: (
    action: ((result: EditorImageResult) => void | Promise<void>) | undefined,
    closeAfter: boolean
  ) => Promise<void>
}

interface DialogContentProps {
  state: ImageEditorDialogState
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>
  workspaceRef: React.RefCallback<HTMLDivElement>
  alt: string
  onExport?: (result: EditorImageResult) => void | Promise<void>
  onApply?: (result: EditorImageResult) => void | Promise<void>
  onClose: () => void
}

function EditorStatus({ failed }: { failed: boolean }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <div className="flex min-h-48 flex-col items-center justify-center gap-2 text-sm text-white/70">
      {failed ? (
        <ImageOff className="size-8" />
      ) : (
        <LoaderCircle className="size-8 animate-spin" />
      )}
      <span>{t(failed ? "imageEditorLoadFailed" : "imageEditorLoading")}</span>
    </div>
  )
}

function EditorHeader({ alt }: { alt: string }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <header className="flex h-12 min-w-0 items-center gap-3 border-b border-white/10 bg-zinc-950/90 px-3">
      <DialogPrimitive.Title className="min-w-0 flex-1 truncate text-sm font-medium">
        {alt || t("imageEditorTitle")}
      </DialogPrimitive.Title>
      <DialogPrimitive.Close asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="rounded-md text-white/80 hover:bg-white/10 hover:text-white"
          aria-label={t("imageEditorClose")}
        >
          <X />
        </Button>
      </DialogPrimitive.Close>
    </header>
  )
}

function EditorWorkspace({
  state,
  canvasRef,
  workspaceRef,
}: Pick<DialogContentProps, "state" | "canvasRef" | "workspaceRef">) {
  const editor = state.editor
  return (
    <div
      ref={workspaceRef}
      className="min-h-0 overflow-auto overscroll-contain"
    >
      <div
        className={cn(
          "grid min-h-full min-w-full place-items-center p-4",
          !state.ready && "h-full"
        )}
      >
        {state.image && state.stage ? (
          <ImageEditorCanvas
            ref={canvasRef}
            image={state.image}
            size={state.stage}
            displayScale={state.displayScale}
            snapshot={editor.snapshot}
            tool={editor.tool}
            color={editor.color}
            strokeWidth={editor.strokeWidth}
            text={editor.text}
            selectedId={editor.selectedId}
            onSelect={editor.setSelectedId}
            onCommit={editor.commit}
          />
        ) : (
          <EditorStatus failed={state.failed} />
        )}
      </div>
    </div>
  )
}

function EditorFooter({
  state,
  onExport,
  onApply,
  onClose,
}: Pick<DialogContentProps, "state" | "onExport" | "onApply" | "onClose">) {
  const editor = state.editor
  return (
    <footer className="flex min-h-14 items-center justify-center border-t border-white/10 bg-zinc-950/90 p-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] text-foreground">
      <ImageEditorToolbar
        tool={editor.tool}
        color={editor.color}
        strokeWidth={editor.strokeWidth}
        text={editor.text}
        zoom={editor.zoom}
        selectedId={editor.selectedId}
        canUndo={editor.canUndo}
        canRedo={editor.canRedo}
        ready={state.ready}
        busy={state.busy}
        canExport={Boolean(onExport)}
        canApply={Boolean(onApply)}
        onToolChange={state.handleToolChange}
        onColorChange={editor.setColor}
        onStrokeWidthChange={editor.setStrokeWidth}
        onTextChange={editor.setText}
        onZoomChange={editor.setZoom}
        onUndo={editor.undo}
        onRedo={editor.redo}
        onDelete={editor.removeSelected}
        onClear={editor.clearEdits}
        onExport={() => void state.runAction(onExport, false)}
        onApply={() => void state.runAction(onApply, true)}
        onClose={onClose}
      />
    </footer>
  )
}

export function ImageEditorDialogContent({
  state,
  canvasRef,
  workspaceRef,
  alt,
  onExport,
  onApply,
  onClose,
}: DialogContentProps) {
  return (
    <>
      <EditorHeader alt={alt} />
      <EditorWorkspace
        state={state}
        canvasRef={canvasRef}
        workspaceRef={workspaceRef}
      />
      <EditorFooter
        state={state}
        onExport={onExport}
        onApply={onApply}
        onClose={onClose}
      />
    </>
  )
}
