const MIB = 1024 * 1024
export const LIBRARY_TRANSFER_LIMIT = 1024 * MIB
const IMAGE_EXTENSIONS = new Set([
  "jpg",
  "jpeg",
  "png",
  "webp",
  "bmp",
  "tiff",
  "ico",
  "dib",
  "icns",
  "sgi",
  "jp2",
])
const FILE_LIMITS: Record<string, number> = {
  pdf: LIBRARY_TRANSFER_LIMIT,
  doc: 350 * MIB,
  docx: 350 * MIB,
  epub: 350 * MIB,
  html: 350 * MIB,
  htm: 350 * MIB,
  ppt: 200 * MIB,
  pptx: 200 * MIB,
  md: 10 * MIB,
  markdown: 10 * MIB,
  txt: 10 * MIB,
  vtt: 10 * MIB,
  mp4: 512 * MIB,
  mkv: 512 * MIB,
  avi: 512 * MIB,
  mov: 512 * MIB,
  wmv: 512 * MIB,
  csv: LIBRARY_TRANSFER_LIMIT,
  xlsx: LIBRARY_TRANSFER_LIMIT,
  jsonl: LIBRARY_TRANSFER_LIMIT,
  bitable: LIBRARY_TRANSFER_LIMIT,
}

export const LIBRARY_ACCEPT = [...Object.keys(FILE_LIMITS), ...IMAGE_EXTENSIONS]
  .map((extension) => `.${extension}`)
  .join(",")

export type FileRuleFailure = {
  key:
    | "emptyFile"
    | "unsupportedFile"
    | "largeFile"
    | "imageDimensions"
    | "invalidName"
    | "tooManyFiles"
  limit?: number
}

export async function validateLibraryFile(
  file: File
): Promise<FileRuleFailure | null> {
  const extension = file.name.split(".").pop()?.toLowerCase() ?? ""
  const limit = IMAGE_EXTENSIONS.has(extension)
    ? 8 * MIB
    : FILE_LIMITS[extension]
  if (!limit || file.type.startsWith("audio/"))
    return { key: "unsupportedFile" }
  if (!file.size) return { key: "emptyFile" }
  if (file.size > limit) return { key: "largeFile", limit: limit / MIB }
  if (
    Array.from(file.name).length > 180 ||
    /[/\\\x00-\x1f\x7f]/.test(file.name)
  )
    return { key: "invalidName" }
  if (!IMAGE_EXTENSIONS.has(extension)) return null
  const size = await imageDimensions(file)
  if (!size) return null
  const [width, height] = size
  const ratio = width / height
  return width < 10 ||
    height < 10 ||
    width > 6000 ||
    height > 6000 ||
    ratio < 0.01 ||
    ratio > 100
    ? { key: "imageDimensions" }
    : null
}

function imageDimensions(file: File): Promise<[number, number] | null> {
  return new Promise((resolve) => {
    const url = URL.createObjectURL(file)
    const image = new Image()
    const finish = (size: [number, number] | null) => {
      URL.revokeObjectURL(url)
      resolve(size)
    }
    image.onload = () => finish([image.naturalWidth, image.naturalHeight])
    image.onerror = () => finish(null)
    image.src = url
  })
}
