export type BinaryPreviewKind = "pdf" | "video" | "audio" | "model"

export function binaryPreviewKind(
  path: string | null | undefined
): BinaryPreviewKind | null {
  if (!path) return null
  const extension = path.split(/[?#]/)[0].split(".").pop()?.toLowerCase()
  if (extension === "pdf") return "pdf"
  if (extension === "glb" || extension === "gltf") return "model"
  if (["mp4", "webm", "mov", "m4v", "ogv", "mkv"].includes(extension ?? ""))
    return "video"
  if (
    ["mp3", "wav", "ogg", "m4a", "aac", "flac", "opus"].includes(
      extension ?? ""
    )
  )
    return "audio"
  return null
}
