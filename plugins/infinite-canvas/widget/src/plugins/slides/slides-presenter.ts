import type { SlideDeck } from "./types.js"
import { validateDeck } from "./types.js"
import { sanitizeHtml } from "../html/html-actions.js"

export function presentSlides(container: HTMLElement, deck: SlideDeck, onExit: () => void): () => void {
  validateDeck(deck)
  const index = Math.max(0, deck.pages.findIndex((page) => page.id === deck.activePageId))
  let active = index
  const render = () => { const page = deck.pages[active]!; container.replaceChildren(); const view = document.createElement("article"); view.innerHTML = sanitizeHtml(page.html); container.append(view) }
  const keydown = (event: KeyboardEvent) => { if (event.key === "Escape") { cleanup(); onExit() } else if (event.key === "ArrowRight") { active = Math.min(deck.pages.length - 1, active + 1); render() } else if (event.key === "ArrowLeft") { active = Math.max(0, active - 1); render() } }
  const cleanup = () => { document.removeEventListener("keydown", keydown); container.replaceChildren(); container.remove() }
  document.addEventListener("keydown", keydown)
  render()
  return cleanup
}
