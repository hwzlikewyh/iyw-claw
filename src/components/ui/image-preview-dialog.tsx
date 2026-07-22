"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { Dialog as DialogPrimitive } from "radix-ui"
import {
  ImageEditorDialogContent,
  type ImageEditorDialogState,
} from "@/components/image-editor/image-editor-dialog-content"
import {
  createDefaultCrop,
  fitStageSize,
  type EditorImageResult,
  type EditorTool,
  type ImageEditorCanvasHandle,
  type StageSize,
} from "@/components/image-editor/image-editor-model"
import { useImageEditor } from "@/components/image-editor/use-image-editor"

interface ImagePreviewDialogProps {
  src: string
  alt: string
  open: boolean
  onOpenChange: (open: boolean) => void
  onExport?: (result: EditorImageResult) => void | Promise<void>
  onApply?: (result: EditorImageResult) => void | Promise<void>
}

interface ElementSize {
  width: number
  height: number
}

interface LoadedImageState {
  source: string
  image: HTMLImageElement | null
  failed: boolean
}

const WORKSPACE_PADDING = 32
const MAX_FIT_SCALE = 2

function useElementSize(): [React.RefCallback<HTMLDivElement>, ElementSize] {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0 })
  const observerRef = useRef<ResizeObserver | null>(null)
  const callbackRef = useCallback((element: HTMLDivElement | null) => {
    observerRef.current?.disconnect()
    observerRef.current = null
    if (!element) return
    const update = (bounds: DOMRectReadOnly) =>
      setSize({ width: bounds.width, height: bounds.height })
    update(element.getBoundingClientRect())
    observerRef.current = new ResizeObserver((entries) => {
      const bounds = entries[0]?.contentRect
      if (bounds) update(bounds)
    })
    observerRef.current.observe(element)
  }, [])
  return [callbackRef, size]
}

function useLoadedImage(src: string, open: boolean): LoadedImageState {
  const source = open ? src : ""
  const [state, setState] = useState<LoadedImageState>({
    source: "",
    image: null,
    failed: false,
  })
  useEffect(() => {
    if (!source) return
    let active = true
    const image = new window.Image()
    image.decoding = "async"
    image.onload = () => active && setState({ source, image, failed: false })
    image.onerror = () =>
      active && setState({ source, image: null, failed: true })
    image.src = source
    return () => {
      active = false
      image.onload = null
      image.onerror = null
    }
  }, [source])
  return state.source === source
    ? state
    : { source, image: null, failed: false }
}

function getFitScale(workspace: ElementSize, stage: StageSize | null): number {
  if (!stage || workspace.width === 0 || workspace.height === 0) return 1
  const width = Math.max(1, workspace.width - WORKSPACE_PADDING)
  const height = Math.max(1, workspace.height - WORKSPACE_PADDING)
  return Math.min(MAX_FIT_SCALE, width / stage.width, height / stage.height)
}

function exportResult(
  canvas: ImageEditorCanvasHandle | null,
  alt: string
): EditorImageResult | null {
  const dataUrl = canvas?.exportPng()
  if (!dataUrl) return null
  const comma = dataUrl.indexOf(",")
  if (comma < 0) return null
  const base = alt.replace(/\.[^.]+$/, "").trim() || "image"
  return {
    data: dataUrl.slice(comma + 1),
    mime_type: "image/png",
    name: `${base}-annotated.png`,
  }
}

function useDialogActions({
  editor,
  stage,
  canvasRef,
  props,
  setBusy,
}: {
  editor: ReturnType<typeof useImageEditor>
  stage: StageSize | null
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>
  props: ImagePreviewDialogProps
  setBusy: React.Dispatch<React.SetStateAction<boolean>>
}) {
  const handleToolChange = useCallback(
    (tool: EditorTool) => {
      editor.setTool(tool)
      if (tool !== "crop" || editor.snapshot.crop || !stage) return
      editor.commit({ ...editor.snapshot, crop: createDefaultCrop(stage) })
    },
    [editor, stage]
  )
  const runAction = useCallback(
    async (
      action: ((result: EditorImageResult) => void | Promise<void>) | undefined,
      closeAfter: boolean
    ) => {
      const result = exportResult(canvasRef.current, props.alt)
      if (!result || !action) return
      setBusy(true)
      try {
        await action(result)
        if (closeAfter) props.onOpenChange(false)
      } finally {
        setBusy(false)
      }
    },
    [canvasRef, props, setBusy]
  )
  return { handleToolChange, runAction }
}

function useDialogEditorState(
  props: ImagePreviewDialogProps,
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>,
  workspace: ElementSize
): ImageEditorDialogState {
  const editor = useImageEditor()
  const loaded = useLoadedImage(props.src, props.open)
  const [busy, setBusy] = useState(false)
  const image = loaded.image
  const stage = useMemo(() => (image ? fitStageSize(image) : null), [image])
  const displayScale = getFitScale(workspace, stage) * editor.zoom
  const resetEditor = editor.reset
  useEffect(() => {
    if (props.open) resetEditor()
  }, [props.open, props.src, resetEditor])
  const actions = useDialogActions({
    editor,
    stage,
    canvasRef,
    props,
    setBusy,
  })
  return {
    editor,
    image,
    failed: loaded.failed || !props.src,
    stage,
    displayScale,
    busy,
    ready: image !== null && stage !== null,
    ...actions,
  }
}

function ImagePreviewDialog(props: ImagePreviewDialogProps) {
  const canvasRef = useRef<ImageEditorCanvasHandle>(null)
  const [workspaceRef, workspace] = useElementSize()
  const state = useDialogEditorState(props, canvasRef, workspace)
  return (
    <DialogPrimitive.Root open={props.open} onOpenChange={props.onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 fixed inset-0 z-50 bg-black/85 duration-100" />
        <DialogPrimitive.Content
          className="fixed inset-0 z-50 grid grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-zinc-950/96 text-white outline-none"
          aria-describedby={undefined}
        >
          <ImageEditorDialogContent
            state={state}
            canvasRef={canvasRef}
            workspaceRef={workspaceRef}
            alt={props.alt}
            onExport={props.onExport}
            onApply={props.onApply}
            onClose={() => props.onOpenChange(false)}
          />
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  )
}

export { ImagePreviewDialog }
export type { EditorImageResult }
