import type { AnnotationShape } from "./types.js"

export type AnnotationTool = "select" | "arrow" | "rect" | "ellipse" | "text" | "freehand"

export function createAnnotationToolbar(onTool: (tool: AnnotationTool) => void, onDelete: () => void): HTMLElement {
  const toolbar = document.createElement("div")
  toolbar.setAttribute("role", "toolbar")
  toolbar.style.cssText = "display:flex;gap:4px;padding:4px;background:#181d26;border:1px solid #303845"
  for (const tool of ["select", "arrow", "rect", "ellipse", "text", "freehand"] as AnnotationTool[]) {
    const button = document.createElement("button")
    button.type = "button"
    button.textContent = tool
    button.addEventListener("click", () => onTool(tool))
    toolbar.append(button)
  }
  const remove = document.createElement("button")
  remove.type = "button"
  remove.textContent = "delete"
  remove.addEventListener("click", onDelete)
  toolbar.append(remove)
  return toolbar
}

export function annotationNode(shapes: AnnotationShape[], imageNodeId: string): Record<string, unknown> {
  return { id: `annotation-${imageNodeId}`, type: "iyw:annotation-layer", x: 0, y: 0, width: 1, height: 1, metadata: { imageNodeId, shapes } }
}
