"use client"

import { useEffect, useRef, type RefObject } from "react"
import type Konva from "konva"
import { Rect, Transformer } from "react-konva"
import type { CropRegion, StageSize } from "./image-editor-model"

interface CropOverlayProps {
  crop: CropRegion
  size: StageSize
  active: boolean
  onCommit: (crop: CropRegion) => void
}

const CROP_COLOR = "#10b981"
const CROP_MIN_SIZE = 24

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), maximum)
}

function clampCrop(crop: CropRegion, size: StageSize): CropRegion {
  const width = clamp(crop.width, CROP_MIN_SIZE, size.width)
  const height = clamp(crop.height, CROP_MIN_SIZE, size.height)
  return {
    x: clamp(crop.x, 0, size.width - width),
    y: clamp(crop.y, 0, size.height - height),
    width,
    height,
  }
}

function CropShade({ crop, size }: Pick<CropOverlayProps, "crop" | "size">) {
  const shade = "rgba(0, 0, 0, 0.48)"
  const bottom = crop.y + crop.height
  const right = crop.x + crop.width
  return (
    <>
      <Rect width={size.width} height={crop.y} fill={shade} listening={false} />
      <Rect
        y={bottom}
        width={size.width}
        height={size.height - bottom}
        fill={shade}
        listening={false}
      />
      <Rect
        x={0}
        y={crop.y}
        width={crop.x}
        height={crop.height}
        fill={shade}
        listening={false}
      />
      <Rect
        x={right}
        y={crop.y}
        width={size.width - right}
        height={crop.height}
        fill={shade}
        listening={false}
      />
    </>
  )
}

function useCropTransformer(
  active: boolean,
  crop: CropRegion,
  cropRef: RefObject<Konva.Rect | null>,
  transformerRef: RefObject<Konva.Transformer | null>
) {
  useEffect(() => {
    const transformer = transformerRef.current
    const node = cropRef.current
    if (!transformer || !node) return
    transformer.nodes(active ? [node] : [])
    transformer.getLayer()?.batchDraw()
  }, [active, crop, cropRef, transformerRef])
}

function CropBox({
  props,
  cropRef,
  onTransformEnd,
}: {
  props: CropOverlayProps
  cropRef: RefObject<Konva.Rect | null>
  onTransformEnd: () => void
}) {
  const { crop, size, active, onCommit } = props
  return (
    <Rect
      ref={cropRef}
      {...crop}
      draggable={active}
      listening={active}
      dragBoundFunc={(position) => ({
        x: clamp(position.x, 0, size.width - crop.width),
        y: clamp(position.y, 0, size.height - crop.height),
      })}
      fill="rgba(16, 185, 129, 0.06)"
      stroke={CROP_COLOR}
      strokeWidth={2}
      onDragEnd={(event) =>
        onCommit(clampCrop({ ...crop, ...event.target.position() }, size))
      }
      onTransformEnd={onTransformEnd}
    />
  )
}

function CropTransformer({
  size,
  transformerRef,
}: {
  size: StageSize
  transformerRef: RefObject<Konva.Transformer | null>
}) {
  return (
    <Transformer
      ref={transformerRef}
      rotateEnabled={false}
      flipEnabled={false}
      anchorFill={CROP_COLOR}
      anchorStroke="#ffffff"
      anchorSize={10}
      borderStroke={CROP_COLOR}
      boundBoxFunc={(_, box) => ({
        ...box,
        ...clampCrop(box, size),
        rotation: 0,
      })}
    />
  )
}

export function ImageEditorCrop(props: CropOverlayProps) {
  const cropRef = useRef<Konva.Rect>(null)
  const transformerRef = useRef<Konva.Transformer>(null)
  useCropTransformer(props.active, props.crop, cropRef, transformerRef)
  const commitTransform = () => {
    const node = cropRef.current
    if (!node) return
    const next = clampCrop(
      {
        x: node.x(),
        y: node.y(),
        width: node.width() * node.scaleX(),
        height: node.height() * node.scaleY(),
      },
      props.size
    )
    node.scale({ x: 1, y: 1 })
    props.onCommit(next)
  }
  return (
    <>
      <CropShade crop={props.crop} size={props.size} />
      <CropBox
        props={props}
        cropRef={cropRef}
        onTransformEnd={commitTransform}
      />
      <CropTransformer size={props.size} transformerRef={transformerRef} />
    </>
  )
}
