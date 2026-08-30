import type { SlideDeck } from "./types.js"
import { validateDeck } from "./types.js"
import { sanitizeHtml } from "../html/html-actions.js"
import type { AssetClient, AssetReference } from "../../asset-client.js"

export function exportDeckHtml(deck: SlideDeck): string {
  validateDeck(deck)
  const pages = deck.pages.map((page) => `<section data-page="${escapeHtml(page.id)}"><h1>${escapeHtml(page.title)}</h1>${sanitizeHtml(page.html)}<aside>${escapeHtml(page.notes)}</aside></section>`).join("")
  const theme = `background:${escapeCss(deck.theme.background)};color:${escapeCss(deck.theme.foreground)};--accent:${escapeCss(deck.theme.accent)}`
  return `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${escapeHtml(deck.title)}</title><style>body{margin:0;font-family:system-ui,sans-serif;${theme}}main{display:grid;gap:32px;padding:32px}section{min-height:360px;padding:28px;border:1px solid var(--accent);break-inside:avoid}aside{margin-top:24px;opacity:.7;white-space:pre-wrap}</style><main>${pages}</main>`
}

export async function exportDeckPages(deck: SlideDeck, assets: AssetClient): Promise<{ pages: Array<{ pageId: string; asset: AssetReference }>; missing: string[] }> {
  validateDeck(deck)
  const result: Array<{ pageId: string; asset: AssetReference }> = []
  const missing: string[] = []
  for (const page of deck.pages) {
    try { result.push({ pageId: page.id, asset: await assets.upload(await rasterizePage(deck, page), `slide-${page.id}.png`, "image/png") }) }
    catch { missing.push(page.id) }
  }
  return { pages: result, missing }
}

async function rasterizePage(deck: SlideDeck, page: SlideDeck["pages"][number]): Promise<Blob> {
  const canvas = document.createElement("canvas")
  canvas.width = 1600
  canvas.height = 900
  const context = canvas.getContext("2d")
  if (!context) throw new Error("slide export canvas is unavailable")
  context.fillStyle = deck.theme.background
  context.fillRect(0, 0, canvas.width, canvas.height)
  context.fillStyle = deck.theme.foreground
  context.font = "bold 56px system-ui"
  context.fillText(page.title, 80, 110)
  context.font = "28px system-ui"
  drawWrappedText(context, plainText(page.html), 80, 190, 1440, 42)
  return new Promise((resolve, reject) => canvas.toBlob((blob) => blob ? resolve(blob) : reject(new Error("slide PNG export failed")), "image/png"))
}

function drawWrappedText(context: CanvasRenderingContext2D, value: string, x: number, y: number, width: number, lineHeight: number): void {
  const words = value.split(/\s+/)
  let line = ""
  for (const word of words) {
    const next = line ? `${line} ${word}` : word
    if (context.measureText(next).width > width && line) { context.fillText(line, x, y); y += lineHeight; line = word } else line = next
  }
  if (line) context.fillText(line, x, y)
}

function plainText(value: string): string { return value.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim().slice(0, 8000) }

function escapeHtml(value: string): string { return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character] ?? character) }
function escapeCss(value: string): string { return /^#[0-9a-f]{6}$/i.test(value) ? value : "#101318" }
