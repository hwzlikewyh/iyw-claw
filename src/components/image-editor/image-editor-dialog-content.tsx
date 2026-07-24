"use client"

import dynamic from "next/dynamic"
import { useCallback, useRef } from "react"
import { ImageOff, LoaderCircle } from "lucide-react"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"
import { ImageEditorToolbar } from "./image-editor-toolbar"
import type {
  EditorImageResult,
  EditorTool,
  ImageEditorCanvasHandle,
  ImagePreviewNavigation,
  ImageViewerMode,
  StageSize,
} from "./image-editor-model"
import { useImageEditor } from "./use-image-editor"
import { useImageWorkspaceNavigation } from "./use-image-workspace-navigation"
import {
  ImageViewerHeader,
  ImageViewerNavigation,
  ImageViewerZoomControls,
} from "./image-viewer-controls"

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
  mode: ImageViewerMode
  rotation: number
  handleModeChange: (mode: ImageViewerMode) => void
  handleRotate: (delta: number) => void
  handleToolChange: (tool: EditorTool) => void
  runAction: (
    action: ((result: EditorImageResult) => void | Promise<void>) | undefined,
    closeAfter: boolean
  ) => Promise<void>
}

interface DialogContentProps {
  state: ImageEditorDialogState
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>
  onCanvasReadyChange: (ready: boolean) => void
  workspaceRef: React.RefCallback<HTMLDivElement>
  alt: string
  navigation?: ImagePreviewNavigation
  onDownload: () => void
  onApply?: (result: EditorImageResult) => void | Promise<void>
  onSendToChat?: (result: EditorImageResult) => void | Promise<void>
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

type EditorWorkspaceProps = Pick<
  DialogContentProps,
  "state" | "canvasRef" | "onCanvasReadyChange" | "workspaceRef" | "navigation"
>

function EditorWorkspace({
  state,
  canvasRef,
  onCanvasReadyChange,
  workspaceRef,
  navigation,
}: EditorWorkspaceProps) {
  const editor = state.editor
  const elementRef = useRef<HTMLDivElement>(null)
  const navigationHandlers = useImageWorkspaceNavigation(elementRef, {
    ready: state.ready,
    busy: state.busy,
    setZoom: editor.setZoom,
  })
  const setWorkspaceRef = useCallback(
    (element: HTMLDivElement | null) => {
      elementRef.current = element
      workspaceRef(element)
    },
    [workspaceRef]
  )
  return (
    <div className="relative min-h-0">
      <div
        ref={setWorkspaceRef}
        onPointerDown={navigationHandlers.onPointerDown}
        onPointerMove={navigationHandlers.onPointerMove}
        onPointerUp={navigationHandlers.endPan}
        onPointerCancel={navigationHandlers.endPan}
        onAuxClick={(event) => event.button === 1 && event.preventDefault()}
        className={cn(
          "h-full overflow-auto overscroll-contain",
          navigationHandlers.panning && "cursor-grabbing select-none"
        )}
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
              onReadyChange={onCanvasReadyChange}
              image={state.image}
              size={state.stage}
              displayScale={state.displayScale}
              displayRotation={state.mode === "view" ? state.rotation : 0}
              rotation={state.rotation}
              snapshot={editor.snapshot}
              tool={editor.tool}
              toolRevision={editor.toolRevision}
              color={editor.color}
              strokeWidth={editor.strokeWidth}
              selectedId={editor.selectedId}
              onSelect={editor.setSelectedId}
              onToolChange={state.handleToolChange}
              onCommit={editor.commit}
            />
          ) : (
            <EditorStatus failed={state.failed} />
          )}
        </div>
      </div>
      {state.mode === "view" ? (
        <ImageViewerNavigation
          navigation={navigation}
          disabled={!state.ready || state.busy}
        />
      ) : null}
    </div>
  )
}

function ViewerFooter({ state }: { state: ImageEditorDialogState }) {
  return (
    <footer className="flex min-h-14 items-center justify-center border-t border-white/8 bg-zinc-950/90 p-2 pb-[max(0.5rem,env(safe-area-inset-bottom))]">
      <ImageViewerZoomControls
        zoom={state.editor.zoom}
        ready={state.ready && !state.busy}
        onZoomChange={state.editor.setZoom}
      />
    </footer>
  )
}

function AnnotationFooter({
  state,
  onApply,
  onSendToChat,
}: Pick<DialogContentProps, "state" | "onApply" | "onSendToChat">) {
  const editor = state.editor
  return (
    <footer className="flex min-h-14 min-w-0 items-center justify-center overflow-hidden border-t border-white/8 bg-zinc-950/90 p-2 pb-[max(0.5rem,env(safe-area-inset-bottom))] text-foreground">
      <ImageEditorToolbar
        tool={editor.tool}
        color={editor.color}
        strokeWidth={editor.strokeWidth}
        zoom={editor.zoom}
        selectedId={editor.selectedId}
        canUndo={editor.canUndo}
        canRedo={editor.canRedo}
        hasEdits={
          editor.snapshot.annotations.length > 0 ||
          editor.snapshot.crop !== null
        }
        ready={state.ready}
        busy={state.busy}
        canApply={Boolean(onApply)}
        canSendToChat={Boolean(onSendToChat)}
        onToolChange={state.handleToolChange}
        onColorChange={editor.setColor}
        onStrokeWidthChange={editor.setStrokeWidth}
        onZoomChange={editor.setZoom}
        onUndo={editor.undo}
        onRedo={editor.redo}
        onDelete={editor.removeSelected}
        onClear={editor.clearEdits}
        onApply={() => void state.runAction(onApply, true)}
        onSendToChat={() => void state.runAction(onSendToChat, true)}
        onDone={() => state.handleModeChange("view")}
      />
    </footer>
  )
}

function EditorFooter(
  props: Pick<DialogContentProps, "state" | "onApply" | "onSendToChat">
) {
  return props.state.mode === "view" ? (
    <ViewerFooter state={props.state} />
  ) : (
    <AnnotationFooter {...props} />
  )
}

export function ImageEditorDialogContent({
  state,
  canvasRef,
  onCanvasReadyChange,
  workspaceRef,
  alt,
  navigation,
  onDownload,
  onApply,
  onSendToChat,
  onClose,
}: DialogContentProps) {
  return (
    <>
      <ImageViewerHeader
        title={alt}
        mode={state.mode}
        ready={state.ready}
        busy={state.busy}
        navigation={navigation}
        onModeChange={state.handleModeChange}
        onRotateLeft={() => state.handleRotate(-90)}
        onRotateRight={() => state.handleRotate(90)}
        onDownload={onDownload}
        onClose={onClose}
      />
      <EditorWorkspace
        state={state}
        canvasRef={canvasRef}
        onCanvasReadyChange={onCanvasReadyChange}
        workspaceRef={workspaceRef}
        navigation={navigation}
      />
      <EditorFooter
        state={state}
        onApply={onApply}
        onSendToChat={onSendToChat}
      />
    </>
  )
}
