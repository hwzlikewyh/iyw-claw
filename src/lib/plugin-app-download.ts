import { getShellTransport, getTransport, isDesktop } from "@/lib/transport"

export const MAX_PLUGIN_DOWNLOAD_BYTES = 32 * 1024 * 1024
const MAX_FILENAME_BYTES = 240
const BASE64_GROUP_SIZE = 4
const BASE64_CHUNK_BYTES = 3
const BASE64_CHUNK_LENGTH = 64 * 1024

export interface PluginDownloadFile {
  name: string
  mimeType: string
  size: number
  blob: Blob
  base64?: string
  text?: string
}

export function parsePluginDownload(params: unknown): PluginDownloadFile {
  const entries = record(params).contents
  if (!Array.isArray(entries) || entries.length !== 1)
    throw Error("PluginDownloads.singleFile")
  const entry = record(entries[0])
  if (entry.type !== "resource") throw Error("PluginDownloads.inlineOnly")
  const resource = record(entry.resource)
  const name = downloadName(resource)
  const mimeType =
    typeof resource.mimeType === "string" &&
    /^[\w.+-]+\/[\w.+-]+$/.test(resource.mimeType)
      ? resource.mimeType
      : "application/octet-stream"
  if (typeof resource.blob === "string" && resource.text === undefined) {
    const base64 = resource.blob
    if (
      base64.length >
      Math.ceil(MAX_PLUGIN_DOWNLOAD_BYTES / BASE64_CHUNK_BYTES) *
        BASE64_GROUP_SIZE
    )
      throw Error("PluginDownloads.tooLarge")
    if (
      !base64 ||
      base64.length % BASE64_GROUP_SIZE ||
      !/^[A-Za-z0-9+/]*={0,2}$/.test(base64)
    )
      throw Error("PluginDownloads.invalidData")
    const chunks = []
    for (
      let offset = 0;
      offset < base64.length;
      offset += BASE64_CHUNK_LENGTH
    ) {
      const binary = atob(base64.slice(offset, offset + BASE64_CHUNK_LENGTH))
      chunks.push(Uint8Array.from(binary, (value) => value.charCodeAt(0)))
    }
    const blob = new Blob(chunks, { type: mimeType })
    if (blob.size > MAX_PLUGIN_DOWNLOAD_BYTES)
      throw Error("PluginDownloads.tooLarge")
    return { name, mimeType, blob, size: blob.size, base64 }
  }
  if (typeof resource.text !== "string" || resource.blob !== undefined)
    throw Error("PluginDownloads.invalidData")
  const blob = new Blob([resource.text], { type: mimeType })
  if (blob.size > MAX_PLUGIN_DOWNLOAD_BYTES)
    throw Error("PluginDownloads.tooLarge")
  return { name, mimeType, blob, size: blob.size, text: resource.text }
}

function downloadName(resource: Record<string, unknown>): string {
  const uri = typeof resource.uri === "string" ? resource.uri : ""
  const name =
    typeof resource.name === "string"
      ? resource.name
      : decodeURIComponent(uri.split("/").pop() ?? "")
  if (
    !name ||
    new TextEncoder().encode(name).length > MAX_FILENAME_BYTES ||
    /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069<>:"/\\|?*]/.test(name) ||
    /[. ]$/.test(name) ||
    /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name) ||
    name === "." ||
    name === ".."
  )
    throw Error("PluginDownloads.invalidName")
  return name
}

export async function savePluginDownload(
  file: PluginDownloadFile,
  authorize: () => Promise<boolean>
): Promise<"done" | "started" | "cancelled"> {
  if (isDesktop()) {
    const { save } = await import("@tauri-apps/plugin-dialog")
    const target = await save({ defaultPath: file.name })
    if (!target || !(await authorize())) return "cancelled"
    // 远程工作区也通过当前设备Shell写入用户选定的位置。
    const shell = getShellTransport()
    if (file.base64 !== undefined)
      await shell.call("save_binary_file", {
        path: target,
        dataBase64: file.base64,
      })
    else
      await shell.call("save_text_file", {
        path: target,
        contents: file.text ?? "",
      })
    return "done"
  }
  if (!(await authorize())) return "cancelled"
  const url = URL.createObjectURL(file.blob),
    link = document.createElement("a")
  link.href = url
  link.download = file.name
  document.body.appendChild(link)
  link.click()
  link.remove()
  setTimeout(() => URL.revokeObjectURL(url), 30_000)
  return "started"
}

export type PluginDownloadAuthorize = () => Promise<boolean>
export type PluginDownloadReply = {
  accepted: boolean
  result?: { isError?: boolean; _meta?: Record<string, unknown> }
  error?: { code: number; message: string }
}
export type PluginDownloadHandler = (request: {
  params: unknown
  authorize: PluginDownloadAuthorize
  transport: ReturnType<typeof getTransport>
}) => Promise<PluginDownloadReply>

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {}
}
