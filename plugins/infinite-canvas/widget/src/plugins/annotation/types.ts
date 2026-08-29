export type Point = { x: number; y: number }
export type AnnotationShape =
  | { id: string; type: "arrow"; from: Point; to: Point; color: string; label?: string }
  | { id: string; type: "rect" | "ellipse"; x: number; y: number; width: number; height: number; color: string }
  | { id: string; type: "text"; x: number; y: number; text: string; color: string }
  | { id: string; type: "freehand"; points: Point[]; color: string; width: number }

export function validateAnnotations(shapes: AnnotationShape[]): void {
  if (shapes.length > 200) throw new Error("annotation shape limit exceeded")
  for (const shape of shapes) {
    if (!/^[A-Za-z0-9_-]{1,64}$/.test(shape.id) || !/^#[0-9a-f]{6}$/i.test(shape.color)) throw new Error("annotation shape is invalid")
    if (shape.type === "arrow" && (!validPoint(shape.from) || !validPoint(shape.to))) throw new Error("annotation arrow is invalid")
    if ((shape.type === "rect" || shape.type === "ellipse") && (![shape.x, shape.y, shape.width, shape.height].every(Number.isFinite) || shape.x < 0 || shape.y < 0 || shape.width < 0 || shape.height < 0 || shape.x + shape.width > 1 || shape.y + shape.height > 1)) throw new Error("annotation geometry is invalid")
    if (shape.type === "text" && (shape.text.length > 1000 || !validPoint({ x: shape.x, y: shape.y }))) throw new Error("annotation text is invalid")
    if (shape.type === "freehand" && (shape.points.length < 2 || shape.points.length > 2000 || !Number.isFinite(shape.width) || shape.width <= 0 || !shape.points.every(validPoint))) throw new Error("annotation freehand path is invalid")
  }
}

function validPoint(point: Point): boolean { return Number.isFinite(point.x) && Number.isFinite(point.y) && point.x >= 0 && point.x <= 1 && point.y >= 0 && point.y <= 1 }
