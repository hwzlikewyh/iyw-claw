export type SlidePage = { id: string; title: string; html: string; notes: string }
export type SlideDeck = {
  schemaVersion: 1
  title: string
  theme: { background: string; foreground: string; accent: string }
  pages: SlidePage[]
  activePageId: string
}

export function validateDeck(deck: SlideDeck): void {
  if (deck.schemaVersion !== 1 || !deck.pages.length || !validColor(deck.theme?.background) || !validColor(deck.theme?.foreground) || !validColor(deck.theme?.accent) || !deck.pages.every((page) => /^[A-Za-z0-9_-]{1,64}$/.test(page.id) && page.title.length <= 200 && page.html.length <= 200_000 && page.notes.length <= 20_000)) throw new Error("slide deck is invalid")
  if (new Set(deck.pages.map((page) => page.id)).size !== deck.pages.length || !deck.pages.some((page) => page.id === deck.activePageId)) throw new Error("slide page IDs are invalid")
}

function validColor(value: unknown): value is string { return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) }
