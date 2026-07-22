export type EditorTool =
  | "select"
  | "crop"
  | "rectangle"
  | "ellipse"
  | "arrow"
  | "freehand"
  | "text"

export interface StageSize {
  width: number
  height: number
}

interface AnnotationTransform {
  id: string
  color: string
  strokeWidth: number
  x: number
  y: number
  scaleX: number
  scaleY: number
  rotation: number
}

export interface RectangleAnnotation extends AnnotationTransform {
  kind: "rectangle"
  width: number
  height: number
}

export interface EllipseAnnotation extends AnnotationTransform {
  kind: "ellipse"
  radiusX: number
  radiusY: number
}

export interface ArrowAnnotation extends AnnotationTransform {
  kind: "arrow"
  points: number[]
}

export interface FreehandAnnotation extends AnnotationTransform {
  kind: "freehand"
  points: number[]
}

export interface TextAnnotation extends AnnotationTransform {
  kind: "text"
  text: string
  fontSize: number
}

export type EditorAnnotation =
  | RectangleAnnotation
  | EllipseAnnotation
  | ArrowAnnotation
  | FreehandAnnotation
  | TextAnnotation

export interface CropRegion {
  x: number
  y: number
  width: number
  height: number
}

export interface EditorSnapshot {
  annotations: EditorAnnotation[]
  crop: CropRegion | null
}

export interface EditorImageResult {
  data: string
  mime_type: "image/png"
  name: string
}

export interface ImageEditorCanvasHandle {
  exportPng: () => string | null
}

export interface ImageEditorToolbarProps {
  tool: EditorTool
  color: string
  strokeWidth: number
  text: string
  zoom: number
  selectedId: string | null
  canUndo: boolean
  canRedo: boolean
  ready: boolean
  busy: boolean
  canExport: boolean
  canApply: boolean
  onToolChange: (tool: EditorTool) => void
  onColorChange: (color: string) => void
  onStrokeWidthChange: (width: number) => void
  onTextChange: (text: string) => void
  onZoomChange: (zoom: number) => void
  onUndo: () => void
  onRedo: () => void
  onDelete: () => void
  onClear: () => void
  onExport: () => void
  onApply: () => void
  onClose: () => void
}

export interface AnnotationTransformUpdate {
  x: number
  y: number
  scaleX: number
  scaleY: number
  rotation: number
}

const MAX_STAGE_WIDTH = 1600
const MAX_STAGE_HEIGHT = 1200
const DEFAULT_CROP_RATIO = 0.8
const ID_RADIX = 36

export function createEmptySnapshot(): EditorSnapshot {
  return { annotations: [], crop: null }
}

export function cloneSnapshot(snapshot: EditorSnapshot): EditorSnapshot {
  return {
    annotations: snapshot.annotations.map((annotation) => ({
      ...annotation,
      ...(annotation.kind === "arrow" || annotation.kind === "freehand"
        ? { points: [...annotation.points] }
        : {}),
    })) as EditorAnnotation[],
    crop: snapshot.crop ? { ...snapshot.crop } : null,
  }
}

export function createAnnotationId(): string {
  const random = Math.random().toString(ID_RADIX).slice(2, 8)
  return `annotation-${Date.now().toString(ID_RADIX)}-${random}`
}

export function fitStageSize(image: HTMLImageElement): StageSize {
  const naturalWidth = Math.max(1, image.naturalWidth)
  const naturalHeight = Math.max(1, image.naturalHeight)
  const scale = Math.min(
    1,
    MAX_STAGE_WIDTH / naturalWidth,
    MAX_STAGE_HEIGHT / naturalHeight
  )
  return {
    width: Math.round(naturalWidth * scale),
    height: Math.round(naturalHeight * scale),
  }
}

export function createDefaultCrop(size: StageSize): CropRegion {
  const width = size.width * DEFAULT_CROP_RATIO
  const height = size.height * DEFAULT_CROP_RATIO
  return {
    x: (size.width - width) / 2,
    y: (size.height - height) / 2,
    width,
    height,
  }
}

export function replaceAnnotation(
  snapshot: EditorSnapshot,
  id: string,
  update: AnnotationTransformUpdate
): EditorSnapshot {
  return {
    ...snapshot,
    annotations: snapshot.annotations.map((annotation) =>
      annotation.id === id ? { ...annotation, ...update } : annotation
    ),
  }
}
