export const CHAT_IMAGE_SOURCE_MAX_BYTES = 100 * 1024 * 1024
export const CHAT_IMAGE_DERIVED_MAX_BYTES = 10 * 1024 * 1024
export const CHAT_IMAGE_MAX_EDGE = 2048

const RESIZE_FACTOR = 0.75
const MAX_RESIZE_ATTEMPTS = 6
const JPEG_QUALITIES = [0.85, 0.7, 0.55, 0.4]

export interface PreparedChatImage {
  data: string
  mimeType: string
  name: string
  sourceBytes: number
  derivedBytes: number
  width: number
  height: number
}

interface BrowserImageSource {
  value: CanvasImageSource
  width: number
  height: number
  dispose: () => void
}

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer)
  let binary = ""
  const chunkSize = 0x8000
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize))
  }
  return btoa(binary)
}

function loadHtmlImage(file: File): Promise<BrowserImageSource> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file)
    const image = new window.Image()
    image.onload = () =>
      resolve({
        value: image,
        width: image.naturalWidth,
        height: image.naturalHeight,
        dispose: () => URL.revokeObjectURL(url),
      })
    image.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error("Unable to decode image"))
    }
    image.src = url
  })
}

async function loadImageSource(file: File): Promise<BrowserImageSource> {
  if (typeof createImageBitmap !== "function") return loadHtmlImage(file)
  const bitmap = await createImageBitmap(file, {
    imageOrientation: "from-image",
  })
  return {
    value: bitmap,
    width: bitmap.width,
    height: bitmap.height,
    dispose: () => bitmap.close(),
  }
}

function targetDimensions(width: number, height: number): [number, number] {
  const scale = Math.min(1, CHAT_IMAGE_MAX_EDGE / Math.max(width, height))
  return [
    Math.max(1, Math.round(width * scale)),
    Math.max(1, Math.round(height * scale)),
  ]
}

function canvasBlob(
  canvas: HTMLCanvasElement,
  mimeType: string,
  quality?: number
): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) =>
        blob ? resolve(blob) : reject(new Error("Unable to encode image")),
      mimeType,
      quality
    )
  })
}

async function encodeCanvas(
  canvas: HTMLCanvasElement,
  usePng: boolean
): Promise<Blob | null> {
  if (usePng) {
    const blob = await canvasBlob(canvas, "image/png")
    return blob.size <= CHAT_IMAGE_DERIVED_MAX_BYTES ? blob : null
  }
  for (const quality of JPEG_QUALITIES) {
    const blob = await canvasBlob(canvas, "image/jpeg", quality)
    if (blob.size <= CHAT_IMAGE_DERIVED_MAX_BYTES) return blob
  }
  return null
}

function drawImage(
  canvas: HTMLCanvasElement,
  source: CanvasImageSource,
  width: number,
  height: number
): void {
  canvas.width = width
  canvas.height = height
  const context = canvas.getContext("2d", { alpha: true })
  if (!context) throw new Error("Canvas is unavailable")
  context.clearRect(0, 0, width, height)
  context.drawImage(source, 0, 0, width, height)
}

export async function prepareBrowserChatImage(
  file: File
): Promise<PreparedChatImage> {
  if (file.size === 0) throw new Error("Image data is empty")
  if (file.size > CHAT_IMAGE_SOURCE_MAX_BYTES) {
    throw new Error("Image exceeds the 100 MB source limit")
  }
  const source = await loadImageSource(file)
  try {
    const canvas = document.createElement("canvas")
    const [initialWidth, initialHeight] = targetDimensions(
      source.width,
      source.height
    )
    let width = initialWidth
    let height = initialHeight
    const usePng = file.type.toLowerCase() !== "image/jpeg"
    for (let attempt = 0; attempt < MAX_RESIZE_ATTEMPTS; attempt += 1) {
      drawImage(canvas, source.value, width, height)
      const blob = await encodeCanvas(canvas, usePng)
      if (blob) {
        return {
          data: arrayBufferToBase64(await blob.arrayBuffer()),
          mimeType: blob.type,
          name: file.name || "image",
          sourceBytes: file.size,
          derivedBytes: blob.size,
          width,
          height,
        }
      }
      width = Math.max(1, Math.round(width * RESIZE_FACTOR))
      height = Math.max(1, Math.round(height * RESIZE_FACTOR))
    }
    throw new Error("Image cannot be reduced to the attachment limit")
  } finally {
    source.dispose()
  }
}
