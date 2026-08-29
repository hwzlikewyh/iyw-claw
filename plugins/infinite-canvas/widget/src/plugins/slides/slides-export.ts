import type { SlideDeck } from "./types.js"
import { validateDeck } from "./types.js"
import { sanitizeHtml } from "../html/html-actions.js"

export function exportDeckHtml(deck: SlideDeck): string {
  validateDeck(deck)
  const pages = deck.pages.map((page) => `<section data-page="${escapeHtml(page.id)}"><h1>${escapeHtml(page.title)}</h1>${sanitizeHtml(page.html)}<aside>${escapeHtml(page.notes)}</aside></section>`).join("")
  const theme = `background:${escapeCss(deck.theme.background)};color:${escapeCss(deck.theme.foreground)};--accent:${escapeCss(deck.theme.accent)}`
  return `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escapeHtml(deck.title)}</title><style>body{margin:0;font-family:system-ui,sans-serif;${theme}}main{display:grid;gap:32px;padding:32px}section{min-height:360px;padding:28px;border:1px solid var(--accent);break-inside:avoid}aside{margin-top:24px;opacity:.7;white-space:pre-wrap}</style><main>${pages}</main>`
}

function escapeHtml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character] ?? character) }
function escapeCss(value: string): string { return /^#[0-9a-f]{6}$/i.test(value) ? value : "#101318" }
