"use client"

import type Konva from "konva"
import { Arrow, Ellipse, Line, Rect, Text } from "react-konva"
import type {
  AnnotationTransformUpdate,
  ArrowAnnotation,
  EditorAnnotation,
  EditorTool,
  EllipseAnnotation,
  FreehandAnnotation,
  RectangleAnnotation,
  TextAnnotation,
} from "./image-editor-model"

interface AnnotationNodeProps {
  annotation: EditorAnnotation
  tool: EditorTool
  selected: boolean
  registerNode: (id: string, node: Konva.Node | null) => void
  onSelect: (id: string) => void
  onTransform: (id: string, update: AnnotationTransformUpdate) => void
}

function readTransform(node: Konva.Node): AnnotationTransformUpdate {
  return {
    x: node.x(),
    y: node.y(),
    scaleX: node.scaleX(),
    scaleY: node.scaleY(),
    rotation: node.rotation(),
  }
}

function commonNodeProps(props: AnnotationNodeProps) {
  const { annotation } = props
  const selectable = props.tool === "select"
  return {
    id: annotation.id,
    ref: (node: Konva.Node | null) => props.registerNode(annotation.id, node),
    x: annotation.x,
    y: annotation.y,
    scaleX: annotation.scaleX,
    scaleY: annotation.scaleY,
    rotation: annotation.rotation,
    draggable: selectable,
    listening: selectable,
    shadowColor: props.selected ? annotation.color : undefined,
    shadowBlur: props.selected ? 4 : 0,
    onClick: () => selectable && props.onSelect(annotation.id),
    onTap: () => selectable && props.onSelect(annotation.id),
    onDragEnd: (event: Konva.KonvaEventObject<DragEvent>) =>
      props.onTransform(annotation.id, readTransform(event.target)),
    onTransformEnd: (event: Konva.KonvaEventObject<Event>) =>
      props.onTransform(annotation.id, readTransform(event.target)),
  }
}

type CommonNodeProps = ReturnType<typeof commonNodeProps>
type ShapeProps<T> = { annotation: T; common: CommonNodeProps }

function RectangleNode({
  annotation,
  common,
}: ShapeProps<RectangleAnnotation>) {
  return (
    <Rect
      {...common}
      width={annotation.width}
      height={annotation.height}
      stroke={annotation.color}
      strokeWidth={annotation.strokeWidth}
    />
  )
}

function EllipseNode({ annotation, common }: ShapeProps<EllipseAnnotation>) {
  return (
    <Ellipse
      {...common}
      radiusX={annotation.radiusX}
      radiusY={annotation.radiusY}
      stroke={annotation.color}
      strokeWidth={annotation.strokeWidth}
    />
  )
}

function ArrowNode({ annotation, common }: ShapeProps<ArrowAnnotation>) {
  return (
    <Arrow
      {...common}
      points={annotation.points}
      stroke={annotation.color}
      fill={annotation.color}
      strokeWidth={annotation.strokeWidth}
      pointerLength={annotation.strokeWidth * 4}
      pointerWidth={annotation.strokeWidth * 3}
      lineCap="round"
      lineJoin="round"
    />
  )
}

function FreehandNode({ annotation, common }: ShapeProps<FreehandAnnotation>) {
  return (
    <Line
      {...common}
      points={annotation.points}
      stroke={annotation.color}
      strokeWidth={annotation.strokeWidth}
      lineCap="round"
      lineJoin="round"
      tension={0.2}
    />
  )
}

function TextNode({ annotation, common }: ShapeProps<TextAnnotation>) {
  return (
    <Text
      {...common}
      text={annotation.text}
      fill={annotation.color}
      fontSize={annotation.fontSize}
      fontStyle="bold"
      padding={2}
    />
  )
}

export function ImageEditorAnnotationNode(props: AnnotationNodeProps) {
  const common = commonNodeProps(props)
  const { annotation } = props
  if (annotation.kind === "rectangle") {
    return <RectangleNode annotation={annotation} common={common} />
  }
  if (annotation.kind === "ellipse") {
    return <EllipseNode annotation={annotation} common={common} />
  }
  if (annotation.kind === "arrow") {
    return <ArrowNode annotation={annotation} common={common} />
  }
  if (annotation.kind === "freehand") {
    return <FreehandNode annotation={annotation} common={common} />
  }
  return <TextNode annotation={annotation} common={common} />
}
