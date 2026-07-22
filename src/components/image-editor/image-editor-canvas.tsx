"use client"

import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type RefObject,
} from "react"
import type Konva from "konva"
import { Image as KonvaImage, Layer, Stage, Transformer } from "react-konva"
import { ImageEditorAnnotationNode } from "./image-editor-annotation"
import { ImageEditorCrop } from "./image-editor-crop"
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
  snapshot: EditorSnapshot
  tool: EditorTool
  color: string
  strokeWidth: number
  text: string
  selectedId: string | null
  onSelect: (id: string | null) => void
  onCommit: (snapshot: EditorSnapshot) => void
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
      if (!stage || !uiLayer) return null
      const crop = props.snapshot.crop
      uiLayer.hide()
      uiLayer.batchDraw()
      try {
        return stage.toDataURL({
          x: crop?.x ?? 0,
          y: crop?.y ?? 0,
          width: crop?.width ?? props.size.width,
          height: crop?.height ?? props.size.height,
          pixelRatio: props.image.naturalWidth / props.size.width,
          mimeType: "image/png",
        })
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
  useSelectionTransformer(props, transformerRef, nodeRefs)
  useCanvasExport(forwardedRef, stageRef, uiLayerRef, props)

  const wrapper = {
    width: props.size.width * props.displayScale,
    height: props.size.height * props.displayScale,
  }
  return (
    <div className="shrink-0" style={wrapper}>
      <div
        style={{
          transform: `scale(${props.displayScale})`,
          transformOrigin: "top left",
        }}
      >
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
      </div>
    </div>
  )
})
