import type { AnnotationShape } from "./types.js"

export function annotationSvg(shapes: AnnotationShape[]): string {
  const content = shapes.map((shape) => {
    if (shape.type === "arrow") return `<line x1="${shape.from.x}" y1="${shape.from.y}" x2="${shape.to.x}" y2="${shape.to.y}" stroke="${escapeXml(shape.color)}" />`
    if (shape.type === "rect") return `<rect x="${shape.x}" y="${shape.y}" width="${shape.width}" height="${shape.height}" fill="none" stroke="${escapeXml(shape.color)}" />`
    if (shape.type === "ellipse") return `<ellipse cx="${shape.x + shape.width / 2}" cy="${shape.y + shape.height / 2}" rx="${Math.abs(shape.width) / 2}" ry="${Math.abs(shape.height) / 2}" fill="none" stroke="${escapeXml(shape.color)}" />`
    if (shape.type === "text") return `<text x="${shape.x}" y="${shape.y}" fill="${escapeXml(shape.color)}">${escapeXml(shape.text)}</text>`
    if (shape.type !== "freehand") return ""
    return `<polyline points="${shape.points.map((point) => `${point.x},${point.y}`).join(" ")}" fill="none" stroke="${escapeXml(shape.color)}" stroke-width="${shape.width}" />`
  }).join("")
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1">${content}</svg>`
}

function escapeXml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&apos;" })[character] ?? character) }
