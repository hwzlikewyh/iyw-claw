import { createAnnotationToolbar, type AnnotationTool } from "./annotation-toolbar.js"
import { validateAnnotations, type AnnotationShape, type Point } from "./types.js"

const DEFAULT_COLOR = "#ffb454"

export function renderAnnotationLayer(container: HTMLElement, initialShapes: AnnotationShape[], onChange: (shapes: AnnotationShape[]) => void): () => void {
  const host = document.createElement("div")
  host.dataset.annotationEditor = "true"
  host.style.cssText = "position:absolute;inset:0;pointer-events:auto"
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg")
  svg.setAttribute("viewBox", "0 0 1 1")
  svg.setAttribute("preserveAspectRatio", "none")
  svg.style.cssText = "position:absolute;inset:0;width:100%;height:100%;pointer-events:auto;touch-action:none"
  let activeTool: AnnotationTool = "select"
  const toolbar = createAnnotationToolbar((tool) => { activeTool = tool }, () => removeSelected())
  toolbar.style.cssText += ";position:absolute;left:4px;top:4px;z-index:2"
  host.append(svg, toolbar)
  container.append(host)

  let shapes = structuredClone(initialShapes)
  let selectedId: string | undefined
  let draft: Draft | undefined

  const redraw = () => draw(svg, shapes, selectedId)
  const emit = (next: AnnotationShape[]) => {
    validateAnnotations(next)
    shapes = next
    redraw()
    onChange(structuredClone(next))
  }
  const removeSelected = () => {
    if (!selectedId) return
    const next = shapes.filter((shape) => shape.id !== selectedId)
    selectedId = undefined
    emit(next)
  }
  const pointerdown = (event: PointerEvent) => {
    const point = pointFromEvent(event, svg)
    const target = (event.target as Element).closest<SVGElement>("[data-annotation-id]")
    if (activeTool === "select") {
      selectedId = target?.dataset.annotationId
      redraw()
      return
    }
    if (activeTool === "text") {
      const value = window.prompt("Annotation text")?.trim()
      if (!value) return
      emit([...shapes, { id: shapeId(), type: "text", x: point.x, y: point.y, text: value, color: DEFAULT_COLOR }])
      return
    }
    draft = { tool: activeTool, start: point, points: [point] }
    svg.setPointerCapture(event.pointerId)
  }
  const pointermove = (event: PointerEvent) => {
    if (!draft) return
    draft.points.push(pointFromEvent(event, svg))
    redrawDraft(svg, shapes, selectedId, draft)
  }
  const finish = (event: PointerEvent) => {
    if (!draft) return
    if (svg.hasPointerCapture(event.pointerId)) svg.releasePointerCapture(event.pointerId)
    const current = draft
    draft = undefined
    const end = current.points.at(-1) ?? current.start
    const next = createShape(current.tool, current.start, end, current.points)
    if (next) emit([...shapes, next])
    else redraw()
  }
  const cancel = (event: PointerEvent) => {
    if (svg.hasPointerCapture(event.pointerId)) svg.releasePointerCapture(event.pointerId)
    draft = undefined
    redraw()
  }
  svg.addEventListener("pointerdown", pointerdown)
  svg.addEventListener("pointermove", pointermove)
  svg.addEventListener("pointerup", finish)
  svg.addEventListener("pointercancel", cancel)
  redraw()

  return () => {
    svg.removeEventListener("pointerdown", pointerdown)
    svg.removeEventListener("pointermove", pointermove)
    svg.removeEventListener("pointerup", finish)
    svg.removeEventListener("pointercancel", cancel)
    host.remove()
  }
}

type Draft = { tool: Exclude<AnnotationTool, "select" | "text">; start: Point; points: Point[] }

function draw(svg: SVGSVGElement, shapes: AnnotationShape[], selectedId?: string): void {
  const defs = document.createElementNS(svg.namespaceURI, "defs")
  const marker = document.createElementNS(svg.namespaceURI, "marker")
  marker.id = "annotation-arrow"
  marker.setAttribute("markerWidth", "0.08")
  marker.setAttribute("markerHeight", "0.08")
  marker.setAttribute("refX", "0.06")
  marker.setAttribute("refY", "0.04")
  marker.setAttribute("orient", "auto")
  const tip = document.createElementNS(svg.namespaceURI, "path")
  tip.setAttribute("d", "M0,0 L0.08,0.04 L0,0.08 z")
  tip.setAttribute("fill", DEFAULT_COLOR)
  marker.append(tip)
  defs.append(marker)
  svg.replaceChildren(defs, ...shapes.map((shape) => renderShape(svg, shape, shape.id === selectedId)))
}

function redrawDraft(svg: SVGSVGElement, shapes: AnnotationShape[], selectedId: string | undefined, draft: Draft): void {
  const end = draft.points.at(-1) ?? draft.start
  const preview = createShape(draft.tool, draft.start, end, draft.points)
  draw(svg, preview ? [...shapes, preview] : shapes, selectedId)
}

function renderShape(svg: SVGSVGElement, shape: AnnotationShape, selected: boolean): SVGElement {
  let value: SVGElement
  if (shape.type === "arrow") value = line(svg, shape.from.x, shape.from.y, shape.to.x, shape.to.y, shape.color)
  else if (shape.type === "rect") value = rectangle(svg, shape.x, shape.y, shape.width, shape.height, shape.color)
  else if (shape.type === "ellipse") value = ellipse(svg, shape.x, shape.y, shape.width, shape.height, shape.color)
  else if (shape.type === "text") value = text(svg, shape.x, shape.y, shape.text, shape.color)
  else if (shape.type === "freehand") value = path(svg, shape.points.map((point: Point) => `${point.x},${point.y}`).join(" "), shape.color, shape.width)
  else value = document.createElementNS(svg.namespaceURI, "g") as SVGGElement
  value.dataset.annotationId = shape.id
  value.style.pointerEvents = "auto"
  if (selected) value.setAttribute("stroke-width", shape.type === "freehand" ? `${shape.width * 2}` : "0.012")
  return value
}

function createShape(tool: Draft["tool"], start: Point, end: Point, points: Point[]): AnnotationShape | undefined {
  const id = shapeId()
  if (tool === "arrow") return { id, type: "arrow", from: start, to: end, color: DEFAULT_COLOR }
  if (tool === "rect" || tool === "ellipse") return { id, type: tool, x: Math.min(start.x, end.x), y: Math.min(start.y, end.y), width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y), color: DEFAULT_COLOR }
  if (tool === "freehand" && points.length > 1) return { id, type: "freehand", points: points.slice(0, 2000), color: DEFAULT_COLOR, width: 0.006 }
  return undefined
}

function pointFromEvent(event: PointerEvent, svg: SVGSVGElement): Point {
  const rect = svg.getBoundingClientRect()
  return { x: clamp((event.clientX - rect.left) / Math.max(1, rect.width)), y: clamp((event.clientY - rect.top) / Math.max(1, rect.height)) }
}

function clamp(value: number): number { return Math.max(0, Math.min(1, value)) }
function shapeId(): string { return `shape-${crypto.randomUUID().slice(0, 12)}` }
function line(svg: SVGSVGElement, x1: number, y1: number, x2: number, y2: number, color: string): SVGLineElement { const value = document.createElementNS(svg.namespaceURI, "line") as SVGLineElement; value.setAttribute("x1", `${x1}`); value.setAttribute("y1", `${y1}`); value.setAttribute("x2", `${x2}`); value.setAttribute("y2", `${y2}`); value.setAttribute("stroke", color); value.setAttribute("marker-end", "url(#annotation-arrow)"); return value }
function rectangle(svg: SVGSVGElement, x: number, y: number, width: number, height: number, color: string): SVGRectElement { const value = document.createElementNS(svg.namespaceURI, "rect") as SVGRectElement; value.setAttribute("x", `${x}`); value.setAttribute("y", `${y}`); value.setAttribute("width", `${width}`); value.setAttribute("height", `${height}`); value.setAttribute("fill", "none"); value.setAttribute("stroke", color); return value }
function ellipse(svg: SVGSVGElement, x: number, y: number, width: number, height: number, color: string): SVGEllipseElement { const value = document.createElementNS(svg.namespaceURI, "ellipse") as SVGEllipseElement; value.setAttribute("cx", `${x + width / 2}`); value.setAttribute("cy", `${y + height / 2}`); value.setAttribute("rx", `${Math.abs(width) / 2}`); value.setAttribute("ry", `${Math.abs(height) / 2}`); value.setAttribute("fill", "none"); value.setAttribute("stroke", color); return value }
function text(svg: SVGSVGElement, x: number, y: number, content: string, color: string): SVGTextElement { const value = document.createElementNS(svg.namespaceURI, "text") as SVGTextElement; value.setAttribute("x", `${x}`); value.setAttribute("y", `${y}`); value.setAttribute("fill", color); value.textContent = content; return value }
function path(svg: SVGSVGElement, points: string, color: string, width: number): SVGPolylineElement { const value = document.createElementNS(svg.namespaceURI, "polyline") as SVGPolylineElement; value.setAttribute("points", points); value.setAttribute("fill", "none"); value.setAttribute("stroke", color); value.setAttribute("stroke-width", `${width}`); return value }
