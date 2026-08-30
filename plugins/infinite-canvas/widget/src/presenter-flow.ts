import type { App } from "@modelcontextprotocol/ext-apps"
import { presentSlides } from "./plugins/slides/slides-presenter.js"
import type { SlideDeck } from "./plugins/slides/types.js"
import { validateDeck } from "./plugins/slides/types.js"

export async function openSlidePresenter(app: App, deck: SlideDeck, onExit: () => void): Promise<() => void> {
  validateDeck(deck)
  await app.requestDisplayMode({ mode: "fullscreen" })
  const overlay = document.createElement("div")
  overlay.style.cssText = "position:fixed;inset:0;z-index:100;background:#101318;color:#f4f7fb;padding:48px;overflow:auto"
  document.body.append(overlay)
  const contentCleanup = presentSlides(overlay, deck, () => { overlay.remove(); onExit() })
  return () => { contentCleanup(); overlay.remove(); onExit() }
}
