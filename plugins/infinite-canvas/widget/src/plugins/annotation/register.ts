import type { AnnotationShape } from "./types.js"
import { annotationNode, createAnnotationToolbar } from "./annotation-toolbar.js"

export const annotationPlugin = {
  type: "iyw:annotation-layer",
  createNode: (imageNodeId: string, shapes: AnnotationShape[]) => annotationNode(shapes, imageNodeId),
  createToolbar: createAnnotationToolbar,
}
