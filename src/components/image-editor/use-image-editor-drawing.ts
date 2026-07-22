"use client"

import { useMemo, useRef, useState, type RefObject } from "react"
import type Konva from "konva"
import {
  createAnnotationId,
  type EditorAnnotation,
  type EditorSnapshot,
  type EditorTool,
} from "./image-editor-model"

interface Point {
  x: number
  y: number
}

interface DrawingOptions {
  stageRef: RefObject<Konva.Stage | null>
  snapshot: EditorSnapshot
  tool: EditorTool
  color: string
  strokeWidth: number
  text: string
  onSelect: (id: string | null) => void
  onCommit: (snapshot: EditorSnapshot) => void
}

const MIN_DRAW_SIZE = 3
const TEXT_FONT_SIZE = 24

function baseAnnotation(
  kind: EditorAnnotation["kind"],
  options: DrawingOptions
) {
  return {
    id: createAnnotationId(),
    kind,
    color: options.color,
    strokeWidth: options.strokeWidth,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
  }
}

function createDraft(
  options: DrawingOptions,
  point: Point
): EditorAnnotation | null {
  if (options.tool === "rectangle") {
    return {
      ...baseAnnotation("rectangle", options),
      kind: options.tool,
      x: point.x,
      y: point.y,
      width: 0,
      height: 0,
    }
  }
  if (options.tool === "ellipse") {
    return {
      ...baseAnnotation("ellipse", options),
      kind: options.tool,
      x: point.x,
      y: point.y,
      radiusX: 0,
      radiusY: 0,
    }
  }
  if (options.tool === "arrow") {
    return {
      ...baseAnnotation("arrow", options),
      kind: options.tool,
      points: [point.x, point.y, point.x, point.y],
    }
  }
  if (options.tool === "freehand") {
    return {
      ...baseAnnotation("freehand", options),
      kind: options.tool,
      points: [point.x, point.y],
    }
  }
  return null
}

function updateDraft(
  draft: EditorAnnotation,
  start: Point,
  point: Point
): EditorAnnotation {
  if (draft.kind === "rectangle") {
    return {
      ...draft,
      x: Math.min(start.x, point.x),
      y: Math.min(start.y, point.y),
      width: Math.abs(point.x - start.x),
      height: Math.abs(point.y - start.y),
    }
  }
  if (draft.kind === "ellipse") {
    return {
      ...draft,
      x: (start.x + point.x) / 2,
      y: (start.y + point.y) / 2,
      radiusX: Math.abs(point.x - start.x) / 2,
      radiusY: Math.abs(point.y - start.y) / 2,
    }
  }
  if (draft.kind === "arrow") {
    return { ...draft, points: [start.x, start.y, point.x, point.y] }
  }
  if (draft.kind === "freehand") {
    return { ...draft, points: [...draft.points, point.x, point.y] }
  }
  return draft
}

function usable(draft: EditorAnnotation): boolean {
  if (draft.kind === "rectangle")
    return draft.width >= MIN_DRAW_SIZE && draft.height >= MIN_DRAW_SIZE
  if (draft.kind === "ellipse")
    return draft.radiusX >= MIN_DRAW_SIZE && draft.radiusY >= MIN_DRAW_SIZE
  if (draft.kind === "arrow")
    return (
      Math.hypot(
        draft.points[2] - draft.points[0],
        draft.points[3] - draft.points[1]
      ) >= MIN_DRAW_SIZE
    )
  return draft.kind === "freehand" && draft.points.length >= 4
}

function addText(options: DrawingOptions, point: Point) {
  const text = options.text.trim()
  if (!text) return
  const annotation: EditorAnnotation = {
    ...baseAnnotation("text", options),
    kind: "text",
    x: point.x,
    y: point.y,
    text,
    fontSize: TEXT_FONT_SIZE,
  }
  options.onCommit({
    ...options.snapshot,
    annotations: [...options.snapshot.annotations, annotation],
  })
  options.onSelect(annotation.id)
}

export function useImageEditorDrawing(options: DrawingOptions) {
  const startRef = useRef<Point | null>(null)
  const draftRef = useRef<EditorAnnotation | null>(null)
  const [draft, setDraft] = useState<EditorAnnotation | null>(null)
  const point = () => options.stageRef.current?.getPointerPosition() ?? null
  const down = (event: Konva.KonvaEventObject<Event>) => {
    const background =
      event.target === event.target.getStage() ||
      event.target.name() === "background"
    if (!background) return
    const position = point()
    if (!position) return
    if (options.tool === "select") return options.onSelect(null)
    if (options.tool === "text") return addText(options, position)
    const next = createDraft(options, position)
    if (!next) return
    startRef.current = position
    draftRef.current = next
    setDraft(next)
    options.onSelect(null)
  }
  const move = () => {
    const position = point()
    if (!draftRef.current || !position || !startRef.current) return
    const next = updateDraft(draftRef.current, startRef.current, position)
    draftRef.current = next
    setDraft(next)
  }
  const up = () => {
    const current = draftRef.current
    if (current && usable(current)) {
      options.onCommit({
        ...options.snapshot,
        annotations: [...options.snapshot.annotations, current],
      })
      options.onSelect(current.id)
    }
    startRef.current = null
    draftRef.current = null
    setDraft(null)
  }
  const annotations = useMemo(
    () =>
      draft
        ? [...options.snapshot.annotations, draft]
        : options.snapshot.annotations,
    [draft, options.snapshot.annotations]
  )
  return { annotations, down, move, up }
}
