// 源格式可以扩展；发给模型的 MIME 仍由准备后的图片内容决定。
export const CHAT_IMAGE_MIME_BY_EXTENSION: Record<string, string> = {
  png: "image/png",
  apng: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  jfif: "image/jpeg",
  jpe: "image/jpeg",
  jif: "image/jpeg",
  jfi: "image/jpeg",
  pjpeg: "image/jpeg",
  pjp: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  bmp: "image/bmp",
  tif: "image/tiff",
  tiff: "image/tiff",
  btf: "image/tiff",
  tf8: "image/tiff",
  ico: "image/vnd.microsoft.icon",
  cur: "image/vnd.microsoft.icon",
  svg: "image/svg+xml",
  heic: "image/heic",
  heif: "image/heic",
  hif: "image/heic",
  heics: "image/heic",
  heifs: "image/heic",
  avif: "image/avif",
  avifs: "image/avif",
  tga: "image/x-tga",
  pbm: "image/x-portable-anymap",
  pgm: "image/x-portable-anymap",
  ppm: "image/x-portable-anymap",
  pnm: "image/x-portable-anymap",
  pam: "image/x-portable-anymap",
  qoi: "image/qoi",
  dds: "image/vnd-ms.dds",
}

const MIME_ALIASES: Record<string, string> = {
  "image/apng": "image/png",
  "image/x-png": "image/png",
  "image/jpg": "image/jpeg",
  "image/pjpeg": "image/jpeg",
  "image/jfif": "image/jpeg",
  "image/x-bmp": "image/bmp",
  "image/x-ms-bmp": "image/bmp",
  "image/x-tiff": "image/tiff",
  "image/x-icon": "image/vnd.microsoft.icon",
  "image/ico": "image/vnd.microsoft.icon",
  "image/heif": "image/heic",
  "image/heic-sequence": "image/heic",
  "image/heif-sequence": "image/heic",
  "image/tga": "image/x-tga",
  "image/x-targa": "image/x-tga",
  "image/x-portable-bitmap": "image/x-portable-anymap",
  "image/x-portable-graymap": "image/x-portable-anymap",
  "image/x-portable-pixmap": "image/x-portable-anymap",
  "image/x-portable-arbitrarymap": "image/x-portable-anymap",
  "image/x-qoi": "image/qoi",
  "image/x-dds": "image/vnd-ms.dds",
}

const SOURCE_MIMES = new Set(Object.values(CHAT_IMAGE_MIME_BY_EXTENSION))

export function sourceImageMime(raw: string): string | null {
  const mime = raw.split(";", 1)[0].trim().toLowerCase()
  const canonical = MIME_ALIASES[mime] ?? mime
  return SOURCE_MIMES.has(canonical) ? canonical : null
}

export function imageMimeTypeFromPath(path: string): string | null {
  const name = path.split(/[/\\]/).pop() ?? ""
  const dot = name.lastIndexOf(".")
  if (dot < 0) return null
  const mime = CHAT_IMAGE_MIME_BY_EXTENSION[name.slice(dot + 1).toLowerCase()]
  return typeof mime === "string" ? mime : null
}

export function imageMimeTypeForFile(file: File): string | null {
  const declared = file.type.split(";", 1)[0].trim().toLowerCase()
  // 未知 image/* 也进入图片准备，明确提示格式错误，不静默当普通附件发送。
  return declared.startsWith("image/")
    ? (sourceImageMime(declared) ??
        imageMimeTypeFromPath(file.name) ??
        declared)
    : imageMimeTypeFromPath(file.name)
}

export async function normalizeChatImageFile(
  file: File,
  mimeHint?: string | null
): Promise<File> {
  const mime = sourceImageMime(imageMimeTypeForFile(file) ?? mimeHint ?? "")
  if (!mime) {
    throw {
      code: "invalid_input",
      message: "Image format is not supported; convert it to PNG or JPEG",
      i18n_key: "errors.chatImage.unsupported",
    }
  }
  const canonical =
    file.type === mime ? file : new File([file], file.name, { type: mime })
  if (mime === "image/avif") {
    const { convertAvifChatImage } = await import("./chat-image-avif")
    return convertAvifChatImage(canonical)
  }
  return canonical
}
