"use client"

import { useRef, useState } from "react"
import { Dialog as DialogPrimitive } from "radix-ui"
import { ImageEditorDialogContent } from "@/components/image-editor/image-editor-dialog-content"
import type {
  EditorImageResult,
  ImageEditorCanvasHandle,
  ImagePreviewNavigation,
} from "@/components/image-editor/image-editor-model"
import {
  useImagePreviewDialog,
  type ImagePreviewDialogProps,
} from "./image-preview-dialog-state"

function ImagePreviewDialogSession(props: ImagePreviewDialogProps) {
  const canvasRef = useRef<ImageEditorCanvasHandle>(null)
  const [canvasReady, setCanvasReady] = useState(false)
  const controller = useImagePreviewDialog(props, canvasRef, canvasReady)
  return (
    <DialogPrimitive.Content
      className="fixed left-1/2 top-1/2 z-50 grid h-[min(88vh,820px)] w-[min(92vw,1200px)] -translate-x-1/2 -translate-y-1/2 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden rounded-md border border-white/10 bg-zinc-950/96 text-white shadow-2xl outline-none"
      aria-describedby={undefined}
    >
      <ImageEditorDialogContent
        state={controller.state}
        canvasRef={canvasRef}
        onCanvasReadyChange={setCanvasReady}
        workspaceRef={controller.workspaceRef}
        alt={props.alt}
        navigation={props.navigation}
        onDownload={controller.onDownload}
        onSendToChat={controller.sendToChatAction}
        onApply={props.onApply}
        onClose={() => props.onOpenChange(false)}
      />
    </DialogPrimitive.Content>
  )
}

function ImagePreviewDialog(props: ImagePreviewDialogProps) {
  return (
    <DialogPrimitive.Root open={props.open} onOpenChange={props.onOpenChange}>
      {props.open ? (
        <DialogPrimitive.Portal>
          <DialogPrimitive.Overlay className="data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 fixed inset-0 z-50 bg-black/55 duration-100" />
          <ImagePreviewDialogSession
            key={props.navigation?.index ?? props.src}
            {...props}
          />
        </DialogPrimitive.Portal>
      ) : null}
    </DialogPrimitive.Root>
  )
}

export { ImagePreviewDialog }
export type {
  EditorImageResult,
  ImagePreviewDialogProps,
  ImagePreviewNavigation,
}
