"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import type { ImageEditorDialogState } from "@/components/image-editor/image-editor-dialog-content"
import {
  exportCanvasImage,
  fetchInlineImage,
  parseInlineImage,
} from "@/components/image-editor/image-editor-export"
import {
  createDefaultCrop,
  fitStageSize,
  type EditorImageResult,
  type EditorTool,
  type ImageEditorCanvasHandle,
  type ImagePreviewNavigation,
  type ImageViewerMode,
  type StageSize,
} from "@/components/image-editor/image-editor-model"
import { useImageEditor } from "@/components/image-editor/use-image-editor"
import { useImageViewerShortcuts } from "@/components/image-editor/use-image-viewer-shortcuts"
import { toErrorMessage } from "@/lib/app-error"
import { downloadImage } from "@/lib/image-download"
import { emitAttachImageToSession } from "@/lib/session-attachment-events"
import { useTabStore } from "@/stores/tab-store"

export interface ImagePreviewDialogProps {
  src: string
  alt: string
  open: boolean
  onOpenChange: (open: boolean) => void
  navigation?: ImagePreviewNavigation
  onExport?: (result: EditorImageResult) => void | Promise<void>
  onApply?: (result: EditorImageResult) => void | Promise<void>
  onSendToChat?: (result: EditorImageResult) => void | Promise<void>
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

const WORKSPACE_PADDING = 96
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
    let retriedWithoutCors = false
    const image = new window.Image()
    image.decoding = "async"
    // Remote images: request CORS so canvas export stays possible. Hosts
    // without CORS headers fail this load — retry without CORS so the
    // preview still displays (export then surfaces a clear cross-origin
    // error instead of breaking the preview).
    if (/^https?:\/\//i.test(source)) image.crossOrigin = "anonymous"
    image.onload = () => active && setState({ source, image, failed: false })
    image.onerror = () => {
      if (!active) return
      if (!retriedWithoutCors && image.crossOrigin !== null) {
        retriedWithoutCors = true
        image.crossOrigin = null
        image.src = source
        return
      }
      setState({ source, image: null, failed: true })
    }
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

function getFitScale(
  workspace: ElementSize,
  stage: StageSize | null,
  rotation: number
): number {
  if (!stage || workspace.width === 0 || workspace.height === 0) return 1
  const width = Math.max(1, workspace.width - WORKSPACE_PADDING)
  const height = Math.max(1, workspace.height - WORKSPACE_PADDING)
  const quarterTurn = Math.abs(rotation) % 180 === 90
  const imageWidth = quarterTurn ? stage.height : stage.width
  const imageHeight = quarterTurn ? stage.width : stage.height
  return Math.min(MAX_FIT_SCALE, width / imageWidth, height / imageHeight)
}

function hasEdits(state: ReturnType<typeof useImageEditor>): boolean {
  return state.snapshot.annotations.length > 0 || state.snapshot.crop !== null
}

interface DialogActionOptions {
  editor: ReturnType<typeof useImageEditor>
  stage: StageSize | null
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>
  props: ImagePreviewDialogProps
  setBusy: React.Dispatch<React.SetStateAction<boolean>>
  onError: (error: unknown) => void
}

function useDialogActions(options: DialogActionOptions) {
  const { editor, stage, canvasRef, props, setBusy, onError } = options
  const t = useTranslations("Folder.chat.messageList")
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
      if (!action) return
      setBusy(true)
      try {
        const edited = hasEdits(editor)
        let result: EditorImageResult | null = null
        if (!edited) {
          // Unedited: ship the original bytes — no PNG re-encode, and no
          // canvas needed (a cross-origin remote image taints the canvas
          // and would make export impossible).
          const inline =
            parseInlineImage(props.src, props.alt) ??
            (await fetchInlineImage(props.src, props.alt).catch(() => null))
          if (inline) {
            result = {
              data: inline.data,
              mime_type: inline.mime_type,
              name: inline.suggestedName,
            }
          }
        }
        if (!result) {
          const outcome = exportCanvasImage(
            canvasRef.current,
            props.alt,
            edited
          )
          if (outcome.status === "ok") {
            result = outcome.result
          } else if (outcome.status === "tainted") {
            throw new Error(t("imageEditorExportTainted"))
          } else {
            throw new Error(t("imageEditorCanvasNotReady"))
          }
        }
        await action(result)
        if (closeAfter) props.onOpenChange(false)
      } catch (error) {
        onError(error)
      } finally {
        setBusy(false)
      }
    },
    [canvasRef, editor, onError, props, setBusy, t]
  )
  return { handleToolChange, runAction }
}

function useViewerState(editor: ReturnType<typeof useImageEditor>) {
  const [mode, setMode] = useState<ImageViewerMode>("view")
  const [rotation, setRotation] = useState(0)
  const handleModeChange = useCallback(
    (nextMode: ImageViewerMode) => {
      setMode(nextMode)
      editor.setSelectedId(null)
      if (nextMode === "annotate") {
        setRotation(0)
        editor.setTool("select")
      }
    },
    [editor]
  )
  const handleRotate = useCallback(
    (delta: number) =>
      setRotation((value) => (((value + delta) % 360) + 360) % 360),
    []
  )
  return { mode, rotation, handleModeChange, handleRotate }
}

function useDialogEditorState(
  props: ImagePreviewDialogProps,
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>,
  canvasReady: boolean,
  workspace: ElementSize,
  onError: (error: unknown) => void
): ImageEditorDialogState {
  const editor = useImageEditor()
  const viewer = useViewerState(editor)
  const loaded = useLoadedImage(props.src, props.open)
  const [busy, setBusy] = useState(false)
  const image = loaded.image
  const stage = useMemo(() => (image ? fitStageSize(image) : null), [image])
  const displayScale =
    getFitScale(workspace, stage, viewer.rotation) * editor.zoom
  const actions = useDialogActions({
    editor,
    stage,
    canvasRef,
    props,
    setBusy,
    onError,
  })
  return {
    editor,
    image,
    failed: loaded.failed || !props.src,
    stage,
    displayScale,
    busy,
    ready: image !== null && stage !== null && canvasReady,
    ...viewer,
    ...actions,
  }
}

function useDefaultActions(props: ImagePreviewDialogProps) {
  const t = useTranslations("Folder.chat.messageList")
  const activeTabId = useTabStore((state) => state.activeTabId)
  const onError = useCallback(
    (error: unknown) => {
      console.error("[image-viewer] image action failed", { error })
      window.alert(
        t("imageEditorActionFailed", { message: toErrorMessage(error) })
      )
    },
    [t]
  )
  const defaultExport = useCallback(async (result: EditorImageResult) => {
    await downloadImage({
      data: result.data,
      mime_type: result.mime_type,
      suggestedName: result.name,
    })
  }, [])
  const defaultSendToChat = useCallback(
    (result: EditorImageResult) => {
      if (!activeTabId) return
      console.info("[image-viewer] adding image to composer", {
        tabId: activeTabId,
        name: result.name,
        mimeType: result.mime_type,
        base64Length: result.data.length,
      })
      emitAttachImageToSession({
        tabId: activeTabId,
        data: result.data,
        mimeType: result.mime_type,
        name: result.name,
      })
    },
    [activeTabId]
  )
  return {
    onError,
    exportAction: props.onExport ?? defaultExport,
    sendToChatAction:
      props.onSendToChat ??
      (activeTabId && !props.onApply ? defaultSendToChat : undefined),
  }
}

export function useImagePreviewDialog(
  props: ImagePreviewDialogProps,
  canvasRef: React.RefObject<ImageEditorCanvasHandle | null>,
  canvasReady: boolean
) {
  const [workspaceRef, workspace] = useElementSize()
  const actions = useDefaultActions(props)
  const state = useDialogEditorState(
    props,
    canvasRef,
    canvasReady,
    workspace,
    actions.onError
  )
  const onDownload = useCallback(() => {
    void state.runAction(actions.exportAction, false)
  }, [actions.exportAction, state])
  useImageViewerShortcuts({
    active: props.open,
    mode: state.mode,
    ready: state.ready && !state.busy,
    zoom: state.editor.zoom,
    navigation: props.navigation,
    onZoomChange: state.editor.setZoom,
    onRotate: state.handleRotate,
    onDownload,
  })
  return { state, workspaceRef, onDownload, ...actions }
}
