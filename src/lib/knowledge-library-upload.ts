import type { Transport } from "@/lib/transport"
import { extractAppCommandError } from "./app-error"
import type { LibraryTarget } from "./knowledge-library"

const UPLOAD_CHUNK_BYTES = 512 * 1024
const BASE64_SLICE_BYTES = 0x8000
const STATUS_INTERVAL_MS = 1000

export type UploadPhase =
  | "receiving"
  | "uploading"
  | "registering"
  | "complete"
  | "failed"
  | "cancelled"
export interface LibraryUploadStatus {
  uploadId: string
  phase: UploadPhase
  receivedBytes: number
  sentBytes: number
  totalBytes: number
  error: string | null
}

export interface LibraryUploadRequest {
  transport: Transport
  target: LibraryTarget
  file: File
  uploadId: string
  signal: AbortSignal
  onStatus: (status: LibraryUploadStatus) => void
}

function reference(request: LibraryUploadRequest) {
  return { uploadId: request.uploadId, scope: request.target.scope }
}

export function releaseLibraryUpload(
  request: Pick<LibraryUploadRequest, "transport" | "uploadId" | "target">
) {
  return request.transport.call<void>("knowledge_upload_abort", {
    input: { uploadId: request.uploadId, scope: request.target.scope },
  })
}

export async function uploadLibraryFile(request: LibraryUploadRequest) {
  const onAbort = () => {
    void releaseLibraryUpload(request).catch(() => {})
  }
  request.signal.addEventListener("abort", onAbort, { once: true })
  try {
    let status = await resumeOrBegin(request)
    request.signal.throwIfAborted()
    if (status.phase === "receiving") status = await receive(request, status)
    request.signal.throwIfAborted()
    if (status.phase === "receiving" || status.phase === "failed") {
      status = await request.transport.call<LibraryUploadStatus>(
        "knowledge_upload_start",
        { input: reference(request) }
      )
    }
    await waitForCloud(request, status)
    await releaseLibraryUpload(request).catch(() => {
      console.warn(
        "[knowledge] Upload finished; temporary session will expire automatically"
      )
    })
  } finally {
    request.signal.removeEventListener("abort", onAbort)
    if (request.signal.aborted)
      await releaseLibraryUpload(request).catch(() => {})
  }
}

async function resumeOrBegin(request: LibraryUploadRequest) {
  request.signal.throwIfAborted()
  try {
    return await request.transport.call<LibraryUploadStatus>(
      "knowledge_upload_status",
      { input: reference(request) }
    )
  } catch (error) {
    if (extractAppCommandError(error)?.code !== "not_found") throw error
  }
  request.signal.throwIfAborted()
  const { target, file } = request
  return request.transport.call<LibraryUploadStatus>("knowledge_upload_begin", {
    input: {
      ...reference(request),
      category: target.category,
      folderId: target.folderId,
      fileName: file.name,
      mimeType: file.type,
      expectedBytes: file.size,
    },
  })
}

async function receive(
  request: LibraryUploadRequest,
  status: LibraryUploadStatus
) {
  request.onStatus(status)
  while (status.receivedBytes < request.file.size) {
    request.signal.throwIfAborted()
    const offset = status.receivedBytes
    const chunk = await request.file
      .slice(offset, offset + UPLOAD_CHUNK_BYTES)
      .arrayBuffer()
    request.signal.throwIfAborted()
    status = await request.transport.call<LibraryUploadStatus>(
      "knowledge_upload_append",
      {
        input: {
          ...reference(request),
          offset,
          dataBase64: encodeChunk(chunk),
        },
      }
    )
    if (
      status.receivedBytes <= offset ||
      status.receivedBytes > request.file.size
    )
      throw new Error("Invalid upload progress")
    request.onStatus(status)
  }
  return status
}

async function waitForCloud(
  request: LibraryUploadRequest,
  initial: LibraryUploadStatus
) {
  let status = initial
  for (;;) {
    request.signal.throwIfAborted()
    request.onStatus(status)
    if (status.phase === "complete") return
    if (status.phase === "failed" || status.phase === "cancelled")
      throw new Error(status.error ?? "Cloud upload failed")
    await delay(request.signal)
    status = await request.transport.call<LibraryUploadStatus>(
      "knowledge_upload_status",
      { input: reference(request) }
    )
  }
}

function encodeChunk(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer)
  let binary = ""
  for (let offset = 0; offset < bytes.length; offset += BASE64_SLICE_BYTES) {
    binary += String.fromCharCode(
      ...bytes.subarray(offset, offset + BASE64_SLICE_BYTES)
    )
  }
  return btoa(binary)
}

function delay(signal: AbortSignal) {
  return new Promise<void>((resolve, reject) => {
    signal.throwIfAborted()
    const finish = () => {
      signal.removeEventListener("abort", abort)
      resolve()
    }
    const timer = window.setTimeout(finish, STATUS_INTERVAL_MS)
    const abort = () => {
      window.clearTimeout(timer)
      reject(signal.reason)
    }
    signal.addEventListener("abort", abort, { once: true })
  })
}
