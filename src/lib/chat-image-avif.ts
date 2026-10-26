import {
  CHAT_IMAGE_INPUT_MAX_BYTES,
  CHAT_IMAGE_SOURCE_MAX_BYTES,
} from "./chat-image"

const MAX_IMAGE_EDGE = 16_384
const MAX_RGBA_BYTES = 512 * 1024 * 1024
const RGBA_BYTES_PER_PIXEL = 4

function decodeFailure() {
  return {
    code: "invalid_input",
    message: "AVIF could not be decoded; the file may be damaged",
    i18n_key: "errors.chatImage.decodeFailed",
  }
}

async function avifBitmap(file: File): Promise<ImageBitmap> {
  try {
    return await createImageBitmap(file)
  } catch {
    throw decodeFailure()
  }
}

function validateBitmap({ width, height }: ImageBitmap) {
  if (
    width === 0 ||
    height === 0 ||
    width > MAX_IMAGE_EDGE ||
    height > MAX_IMAGE_EDGE ||
    width * height * RGBA_BYTES_PER_PIXEL > MAX_RGBA_BYTES
  ) {
    throw {
      code: "invalid_input",
      message: "Image dimensions exceed the decoding limits",
      i18n_key: "errors.chatImage.dimensions",
    }
  }
}

async function canvasPng(canvas: HTMLCanvasElement): Promise<Blob> {
  const png = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(decodeFailure())),
      "image/png"
    )
  )
  if (png.size > CHAT_IMAGE_INPUT_MAX_BYTES) {
    throw {
      code: "invalid_input",
      message: "Converted PNG exceeds the 32 MiB model input limit",
      i18n_key: "errors.chatImage.inputTooLarge",
    }
  }
  return png
}

export async function convertAvifChatImage(file: File): Promise<File> {
  if (file.size === 0 || file.size > CHAT_IMAGE_SOURCE_MAX_BYTES) {
    throw {
      code: "invalid_input",
      message: "Image source must be between 1 byte and 100 MiB",
    }
  }
  const bitmap = await avifBitmap(file)
  const canvas = document.createElement("canvas")
  try {
    validateBitmap(bitmap)
    const { width, height } = bitmap
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext("2d")
    if (!context) throw decodeFailure()
    context.drawImage(bitmap, 0, 0)
    const png = await canvasPng(canvas)
    const stem = file.name.replace(/\.[^.]*$/, "") || "image"
    return new File([png], `${stem}.png`, { type: "image/png" })
  } finally {
    bitmap.close()
    canvas.width = 0
    canvas.height = 0
  }
}
