"use client"

import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type CSSProperties,
  type RefObject,
} from "react"
import type Konva from "konva"
import { Image as KonvaImage, Layer, Stage, Transformer } from "react-konva"
import { ImageEditorAnnotationNode } from "./image-editor-annotation"
import { ImageEditorCrop } from "./image-editor-crop"
import { ImageEditorInlineText } from "./image-editor-inline-text"
import {
  replaceAnnotation,
  type AnnotationTransformUpdate,
  type EditorAnnotation,
  type EditorSnapshot,
  type EditorTool,
  type ImageEditorCanvasHandle,
  type StageSize,
} from "./image-editor-model"
import { useImageEditorDrawing } from "./use-image-editor-drawing"

export interface ImageEditorCanvasProps {
  image: HTMLImageElement
  size: StageSize
  displayScale: number
  displayRotation: number
  rotation: number
  snapshot: EditorSnapshot
  tool: EditorTool
  toolRevision: number
  color: string
  strokeWidth: number
  selectedId: string | null
  onSelect: (id: string | null) => void
  onToolChange: (tool: EditorTool) => void
  onCommit: (snapshot: EditorSnapshot) => void
  onReadyChange: (ready: boolean) => void
}

interface AnnotationLayerProps {
  annotations: EditorAnnotation[]
  props: ImageEditorCanvasProps
  nodeRefs: RefObject<Map<string, Konva.Node>>
}

function AnnotationLayer({
  annotations,
  props,
  nodeRefs,
}: AnnotationLayerProps) {
  const commit = (id: string, update: AnnotationTransformUpdate) => {
    props.onCommit(replaceAnnotation(props.snapshot, id, update))
  }
  const register = (id: string, node: Konva.Node | null) => {
    if (node) nodeRefs.current.set(id, node)
    else nodeRefs.current.delete(id)
  }
  return (
    <Layer>
      <KonvaImage image={props.image} {...props.size} name="background" />
      {annotations.map((annotation) => (
        <ImageEditorAnnotationNode
          key={annotation.id}
          annotation={annotation}
          tool={props.tool}
          selected={annotation.id === props.selectedId}
          registerNode={register}
          onSelect={props.onSelect}
          onTransform={commit}
        />
      ))}
    </Layer>
  )
}

function EditorUiLayer({
  props,
  layerRef,
  transformerRef,
}: {
  props: ImageEditorCanvasProps
  layerRef: RefObject<Konva.Layer | null>
  transformerRef: RefObject<Konva.Transformer | null>
}) {
  return (
    <Layer ref={layerRef}>
      {props.snapshot.crop && (
        <ImageEditorCrop
          crop={props.snapshot.crop}
          size={props.size}
          active={props.tool === "crop"}
          onCommit={(crop) => props.onCommit({ ...props.snapshot, crop })}
        />
      )}
      <Transformer
        ref={transformerRef}
        rotateEnabled={false}
        flipEnabled={false}
        anchorSize={9}
        borderStroke="#3b82f6"
        anchorFill="#3b82f6"
        anchorStroke="#ffffff"
      />
    </Layer>
  )
}

function useSelectionTransformer(
  props: ImageEditorCanvasProps,
  transformerRef: RefObject<Konva.Transformer | null>,
  nodeRefs: RefObject<Map<string, Konva.Node>>
) {
  useEffect(() => {
    const transformer = transformerRef.current
    if (!transformer) return
    const node =
      props.tool === "select" && props.selectedId
        ? nodeRefs.current.get(props.selectedId)
        : null
    transformer.nodes(node ? [node] : [])
    transformer.getLayer()?.batchDraw()
  }, [
    nodeRefs,
    props.selectedId,
    props.snapshot.annotations,
    props.tool,
    transformerRef,
  ])
}

function renderExportCanvas(
  stage: Konva.Stage,
  props: ImageEditorCanvasProps
): HTMLCanvasElement {
  const crop = props.snapshot.crop
  return stage.toCanvas({
    x: crop?.x ?? 0,
    y: crop?.y ?? 0,
    width: crop?.width ?? props.size.width,
    height: crop?.height ?? props.size.height,
    pixelRatio: props.image.naturalWidth / props.size.width,
  })
}

function rotateExportCanvas(
  source: HTMLCanvasElement,
  rotation: number
): HTMLCanvasElement {
  const normalized = ((rotation % 360) + 360) % 360
  if (normalized === 0) return source
  const quarterTurn = normalized === 90 || normalized === 270
  const output = document.createElement("canvas")
  output.width = quarterTurn ? source.height : source.width
  output.height = quarterTurn ? source.width : source.height
  const context = output.getContext("2d")
  if (!context) throw new Error("Cannot create image export canvas")
  context.translate(output.width / 2, output.height / 2)
  context.rotate((normalized * Math.PI) / 180)
  context.drawImage(source, -source.width / 2, -source.height / 2)
  return output
}

function exportStagePng(stage: Konva.Stage, props: ImageEditorCanvasProps) {
  try {
    const rendered = renderExportCanvas(stage, props)
    const output = rotateExportCanvas(rendered, props.rotation)
    const dataUrl = output.toDataURL("image/png")
    return dataUrl
      ? { status: "ok" as const, dataUrl }
      : { status: "tainted" as const }
  } catch (error) {
    if (error instanceof DOMException && error.name === "SecurityError") {
      return { status: "tainted" as const }
    }
    throw error
  }
}

function useCanvasExport(
  forwardedRef: React.ForwardedRef<ImageEditorCanvasHandle>,
  stageRef: RefObject<Konva.Stage | null>,
  uiLayerRef: RefObject<Konva.Layer | null>,
  props: ImageEditorCanvasProps
) {
  useImperativeHandle(forwardedRef, () => ({
    exportPng: () => {
      const stage = stageRef.current
      const uiLayer = uiLayerRef.current
      if (!stage || !uiLayer) return { status: "not-ready" as const }
      uiLayer.hide()
      uiLayer.batchDraw()
      try {
        return exportStagePng(stage, props)
      } finally {
        uiLayer.show()
        uiLayer.batchDraw()
      }
    },
  }))
}

export const ImageEditorCanvas = forwardRef<
  ImageEditorCanvasHandle,
  ImageEditorCanvasProps
>(function ImageEditorCanvas(props, forwardedRef) {
  const stageRef = useRef<Konva.Stage>(null)
  const uiLayerRef = useRef<Konva.Layer>(null)
  const transformerRef = useRef<Konva.Transformer>(null)
  const nodeRefs = useRef(new Map<string, Konva.Node>())
  const drawing = useImageEditorDrawing({ stageRef, ...props })
  const onReadyChange = props.onReadyChange
  useSelectionTransformer(props, transformerRef, nodeRefs)
  useCanvasExport(forwardedRef, stageRef, uiLayerRef, props)
  useEffect(() => {
    onReadyChange(true)
    return () => onReadyChange(false)
  }, [onReadyChange])

  const layout = getCanvasLayout(props)
  return (
    <div className="relative shrink-0" style={layout.wrapper}>
      <div style={layout.canvas}>
        <Stage
          ref={stageRef}
          {...props.size}
          style={{ touchAction: "none" }}
          onMouseDown={drawing.down}
          onMousemove={drawing.move}
          onMouseup={drawing.up}
          onMouseleave={drawing.up}
          onTouchstart={drawing.down}
          onTouchmove={drawing.move}
          onTouchend={drawing.up}
        >
          <AnnotationLayer
            annotations={drawing.annotations}
            props={props}
            nodeRefs={nodeRefs}
          />
          <EditorUiLayer
            props={props}
            layerRef={uiLayerRef}
            transformerRef={transformerRef}
          />
        </Stage>
        {drawing.textDraft ? (
          <ImageEditorInlineText
            draft={drawing.textDraft}
            color={props.color}
            size={props.size}
            onChange={drawing.onTextChange}
            onCommit={drawing.onTextCommit}
            onCancel={drawing.onTextCancel}
          />
        ) : null}
      </div>
    </div>
  )
})

function getCanvasLayout(props: ImageEditorCanvasProps): {
  wrapper: CSSProperties
  canvas: CSSProperties
} {
  const quarterTurn = Math.abs(props.displayRotation) % 180 === 90
  return {
    wrapper: {
      width:
        (quarterTurn ? props.size.height : props.size.width) *
        props.displayScale,
      height:
        (quarterTurn ? props.size.width : props.size.height) *
        props.displayScale,
    },
    canvas: {
      position: "absolute",
      left: "50%",
      top: "50%",
      width: props.size.width,
      height: props.size.height,
      transform: `translate(-50%, -50%) rotate(${props.displayRotation}deg) scale(${props.displayScale})`,
      transformOrigin: "center",
    },
  }
}
