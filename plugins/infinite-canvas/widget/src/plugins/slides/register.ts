import { exportDeckHtml } from "./slides-export.js"
import { presentSlides } from "./slides-presenter.js"
import { slidesNode } from "./slides-node.js"

export const slidesPlugin = { type: "iyw:slides", createNode: slidesNode, present: presentSlides, exportHtml: exportDeckHtml }
