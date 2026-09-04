import { isDesktop } from "@/lib/platform"
import {
  fetchDisplayAsset,
  isDisplayAssetUri,
} from "@/lib/display-asset-source"

/**
 * Save an inline base64 image to user-chosen disk location.
 *
 * Desktop (Tauri): pops the system "Save As" dialog, then writes the
 * decoded bytes to the chosen path via the `save_binary_file` command.
 * Web (browser): triggers a Blob download via an `<a download>` link;
 * the browser uses its own download manager / location.
 *
 * Returns true if a file was written, false if the user cancelled the
 * dialog (desktop only). Throws on actual write failure so the caller
 * can surface an error toast.
 */
export async function downloadImage(opts: {
  data: string
  mime_type: string
  suggestedName: string
  uri?: string | null
}): Promise<boolean> {
  const { data, mime_type, suggestedName, uri } = opts
  if (isDisplayAssetUri(uri)) {
    return downloadBlob(await fetchDisplayAsset(uri!, mime_type), suggestedName)
  }
  if (typeof uri === "string" && /^https?:\/\//i.test(uri)) {
    const response = await fetch(uri)
    if (!response.ok) throw new Error(`HTTP ${response.status}`)
    return downloadBlob(await response.blob(), suggestedName)
  }

  if (isDesktop()) {
    const { save } = await import("@tauri-apps/plugin-dialog")
    const { invoke } = await import("@tauri-apps/api/core")

    const ext = extensionForMime(mime_type)
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Image", extensions: [ext] }],
    })
    if (!path) return false

    await invoke("save_binary_file", { path, dataBase64: data })
    return true
  }

  const bytes = base64ToUint8Array(data)
  return downloadBlob(
    new Blob([bytes as BlobPart], { type: mime_type }),
    suggestedName
  )
}

function downloadBlob(blob: Blob, suggestedName: string): boolean {
  const url = URL.createObjectURL(blob)
  const link = document.createElement("a")
  link.href = url
  link.download = suggestedName
  document.body.append(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
  return true
}

function extensionForMime(mime: string): string {
  const sub = mime.split("/")[1]?.split("+")[0]?.toLowerCase()
  if (!sub) return "png"
  if (sub === "jpeg") return "jpg"
  return sub
}

function base64ToUint8Array(b64: string): Uint8Array {
  const binary = atob(b64)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes
}
