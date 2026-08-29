import { validateAnnotations, type AnnotationShape } from "./types.js"

const MAX_INPUT_BYTES = 20 * 1024 * 1024
const MAX_PIXELS = 16_000_000

export async function flattenAnnotations(imageUrl: string, shapes: AnnotationShape[], sourceBytes?: number): Promise<Blob> {
  validateAnnotations(shapes)
  if (sourceBytes !== undefined && (!Number.isSafeInteger(sourceBytes) || sourceBytes < 1 || sourceBytes > MAX_INPUT_BYTES)) throw new Error("annotation source exceeds 20 MiB limit")
  const image = await loadImage(imageUrl)
  const canvas = document.createElement("canvas")
  canvas.width = image.naturalWidth
  canvas.height = image.naturalHeight
  if (!canvas.width || !canvas.height) throw new Error("annotation source image has no pixels")
  if (canvas.width * canvas.height > MAX_PIXELS) throw new Error("annotation source exceeds 16 megapixel limit")
  const context = canvas.getContext("2d")
  if (!context) throw new Error("canvas 2d context is unavailable")
  context.drawImage(image, 0, 0)
  drawShapes(context, shapes, canvas.width, canvas.height)
  return new Promise((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("annotation PNG export failed")), "image/png"))
}

function drawShapes(context: CanvasRenderingContext2D, shapes: AnnotationShape[], width: number, height: number): void {
  for (const shape of shapes) {
    context.strokeStyle = shape.color
    context.fillStyle = shape.color
    context.lineWidth = shape.type === "freehand" ? shape.width * Math.min(width, height) : 2
    if (shape.type === "arrow") { drawLine(context, shape.from.x * width, shape.from.y * height, shape.to.x * width, shape.to.y * height); continue }
    if (shape.type === "rect") { context.strokeRect(shape.x * width, shape.y * height, shape.width * width, shape.height * height); continue }
    if (shape.type === "ellipse") { context.beginPath(); context.ellipse((shape.x + shape.width / 2) * width, (shape.y + shape.height / 2) * height, Math.abs(shape.width * width / 2), Math.abs(shape.height * height / 2), 0, 0, Math.PI * 2); context.stroke(); continue }
    if (shape.type === "text") { context.font = `${Math.max(12, Math.round(height * 0.03))}px sans-serif`; context.fillText(shape.text, shape.x * width, shape.y * height); continue }
    if (shape.type !== "freehand") continue
    context.beginPath(); shape.points.forEach((point, index) => index ? context.lineTo(point.x * width, point.y * height) : context.moveTo(point.x * width, point.y * height)); context.stroke()
  }
}

function drawLine(context: CanvasRenderingContext2D, fromX: number, fromY: number, toX: number, toY: number): void {
  context.beginPath(); context.moveTo(fromX, fromY); context.lineTo(toX, toY); context.stroke()
  const angle = Math.atan2(toY - fromY, toX - fromX)
  context.beginPath(); context.moveTo(toX, toY); context.lineTo(toX - 10 * Math.cos(angle - Math.PI / 6), toY - 10 * Math.sin(angle - Math.PI / 6)); context.lineTo(toX - 10 * Math.cos(angle + Math.PI / 6), toY - 10 * Math.sin(angle + Math.PI / 6)); context.closePath(); context.fill()
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => { const image = new Image(); image.onload = () => resolve(image); image.onerror = () => reject(new Error("annotation source image could not be decoded")); image.src = url })
}
