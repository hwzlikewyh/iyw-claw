import type { SlideDeck } from "./types.js"

export function slidesNode(deck: SlideDeck, id: string): Record<string, unknown> {
  return { id, type: "iyw:slides", x: 0, y: 0, width: 640, height: 400, metadata: { deck } }
}
