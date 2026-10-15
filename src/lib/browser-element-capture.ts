import type { BrowserElementSummary, BrowserScreenshot } from "./browser-types"

export async function cropBrowserElement(
  screenshot: BrowserScreenshot,
  element: BrowserElementSummary
): Promise<BrowserScreenshot> {
  if (!element.bounds || !element.viewport) return screenshot
  const [left, top, width, height] = element.bounds
  const [viewportWidth, viewportHeight] = element.viewport
  if (viewportWidth <= 0 || viewportHeight <= 0) return screenshot
  const image = new Image()
  image.src = `data:${screenshot.mimeType};base64,${screenshot.data}`
  await image.decode()
  const scaleX = image.naturalWidth / viewportWidth
  const scaleY = image.naturalHeight / viewportHeight
  const x = Math.max(0, left) * scaleX
  const y = Math.max(0, top) * scaleY
  const w = Math.min(viewportWidth, left + width) * scaleX - x
  const h = Math.min(viewportHeight, top + height) * scaleY - y
  if (w <= 0 || h <= 0) return screenshot
  const canvas = document.createElement("canvas")
  canvas.width = Math.max(1, Math.ceil(w))
  canvas.height = Math.max(1, Math.ceil(h))
  const context = canvas.getContext("2d")
  if (!context) throw new Error("Unable to crop the browser screenshot")
  context.drawImage(image, x, y, w, h, 0, 0, canvas.width, canvas.height)
  return {
    data: canvas.toDataURL("image/png").split(",")[1],
    mimeType: "image/png",
  }
}

export function formatBrowserElement(
  element: BrowserElementSummary,
  url: string
): string {
  const html = element.html ?? ""
  const fence = "`".repeat(
    Math.max(
      3,
      ...Array.from(html.matchAll(/`+/g), (match) => match[0].length + 1)
    )
  )
  return [
    url,
    element.selector,
    element.text,
    html ? `${fence}html\n${html}\n${fence}` : null,
  ]
    .filter(Boolean)
    .join("\n\n")
}

export function browserElementImageName(selector: string): string {
  return `${
    selector
      .replace(/[^\p{L}\p{N}_-]+/gu, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 60) || "element"
  }.png`
}
