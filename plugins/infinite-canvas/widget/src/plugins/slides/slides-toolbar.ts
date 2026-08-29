import type { SlideDeck } from "./types.js"

export function createSlidesToolbar(deck: SlideDeck, onPage: (pageId: string) => void, onPresent: () => void): HTMLElement {
  const toolbar = document.createElement("div")
  toolbar.style.cssText = "display:flex;gap:6px;align-items:center"
  for (const page of deck.pages) { const button = document.createElement("button"); button.type = "button"; button.textContent = page.title || page.id; button.addEventListener("click", () => onPage(page.id)); toolbar.append(button) }
  const present = document.createElement("button")
  present.type = "button"
  present.textContent = "Present"
  present.addEventListener("click", onPresent)
  toolbar.append(present)
  return toolbar
}
