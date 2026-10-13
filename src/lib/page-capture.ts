export type PageCapture = { data: string; mimeType: string }
export type CapturePoint = { x: number; y: number }
export type CaptureRegion = CapturePoint & { width: number; height: number }
export type CaptureMark = {
  tool: "box" | "arrow"
  start: CapturePoint
  end: CapturePoint
}

export const MIN_CAPTURE_DRAG = 4
const MARK_COLOR = "#7c3aed"
const MARK_WIDTH = 3
const ARROW_SIZE = 14

export function captureRegion(start: CapturePoint, end: CapturePoint) {
  return {
    x: Math.min(start.x, end.x),
    y: Math.min(start.y, end.y),
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y),
  }
}

export function drawCaptureMark(
  ctx: CanvasRenderingContext2D,
  mark: CaptureMark
) {
  ctx.strokeStyle = MARK_COLOR
  ctx.fillStyle = MARK_COLOR
  ctx.lineWidth = MARK_WIDTH
  if (mark.tool === "box") {
    const region = captureRegion(mark.start, mark.end)
    ctx.strokeRect(region.x, region.y, region.width, region.height)
    return
  }
  const angle = Math.atan2(mark.end.y - mark.start.y, mark.end.x - mark.start.x)
  ctx.beginPath()
  ctx.moveTo(mark.start.x, mark.start.y)
  ctx.lineTo(mark.end.x, mark.end.y)
  ctx.stroke()
  ctx.beginPath()
  ctx.moveTo(mark.end.x, mark.end.y)
  for (const offset of [-Math.PI / 6, Math.PI / 6]) {
    ctx.lineTo(
      mark.end.x - ARROW_SIZE * Math.cos(angle + offset),
      mark.end.y - ARROW_SIZE * Math.sin(angle + offset)
    )
  }
  ctx.closePath()
  ctx.fill()
}

export function loadCapture(capture: PageCapture): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image()
    image.onload = () => resolve(image)
    image.onerror = () =>
      reject(new Error("Could not decode the page screenshot"))
    image.src = `data:${capture.mimeType};base64,${capture.data}`
  })
}

export async function renderPageCapture(options: {
  capture: PageCapture
  marks: CaptureMark[]
  region: CaptureRegion | null
}): Promise<PageCapture> {
  const image = await loadCapture(options.capture)
  const canvas = document.createElement("canvas")
  const region = options.region ?? {
    x: 0,
    y: 0,
    width: image.naturalWidth,
    height: image.naturalHeight,
  }
  canvas.width = Math.max(1, Math.round(region.width))
  canvas.height = Math.max(1, Math.round(region.height))
  const ctx = canvas.getContext("2d")
  if (!ctx) throw new Error("Canvas is unavailable")
  ctx.translate(-region.x, -region.y)
  ctx.drawImage(image, 0, 0)
  options.marks.forEach((mark) => drawCaptureMark(ctx, mark))
  return {
    data: canvas.toDataURL("image/png").split(",")[1],
    mimeType: "image/png",
  }
}
