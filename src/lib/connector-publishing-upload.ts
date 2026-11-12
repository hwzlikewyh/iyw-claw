import {
  connectorUserRequest,
  type ConnectorArtifact,
  type ConnectorUploadTask,
} from "./connector-publishing"
import { connectorFileDigest } from "./connector-upload-digest"

export interface PendingConnectorUpload {
  file: File
  task: ConnectorUploadTask
  transferred: boolean
}
export interface ConnectorUploadOptions {
  signal: AbortSignal
  status: (text: string) => void
  pending?: PendingConnectorUpload
  retain: (pending: PendingConnectorUpload) => void
  onPhase?: (phase: "digest" | "transfer" | "verify") => void
}
export async function uploadConnectorArtifact(
  file: File,
  options: ConnectorUploadOptions
): Promise<ConnectorArtifact> {
  if (!Number.isSafeInteger(file.size) || file.size < 1)
    throw Error("请选择非空且大小有效的连接器文件")
  let pending = options.pending
  if (
    !pending ||
    pending.file !== file ||
    Date.parse(pending.task.expiresAt) <= Date.now()
  ) {
    options.onPhase?.("digest")
    const sha256 = await connectorFileDigest(file, {
      signal: options.signal,
      progress: (done) =>
        options.status(
          `正在校验文件：${Math.round((done / file.size) * 100)}%`
        ),
    })
    options.signal.throwIfAborted()
    const task = await connectorUserRequest<ConnectorUploadTask>(
      "uploads/init",
      { filename: file.name, size: file.size, sha256 }
    )
    pending = { file, task, transferred: false }
    options.retain(pending)
  }
  options.signal.throwIfAborted()
  if (!pending.transferred) {
    options.onPhase?.("transfer")
    options.status("正在上传连接器文件…")
    await transfer(file, pending.task, options.signal)
    pending.transferred = true
    options.retain(pending)
  }
  options.signal.throwIfAborted()
  options.onPhase?.("verify")
  options.status("正在核对实际文件并登记制品…")
  return connectorUserRequest<ConnectorArtifact>("uploads/complete", {
    uploadToken: pending.task.uploadToken,
  })
}

async function transfer(
  file: File,
  task: ConnectorUploadTask,
  signal: AbortSignal
) {
  const url = new URL(task.url)
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    task.method !== "PUT"
  )
    throw Error("受管上传地址无效")
  const headers = Object.fromEntries(
    Object.entries(task.headers || {}).filter(
      ([key]) =>
        ![
          "host",
          "content-length",
          "token",
          "admin-token",
          "authorization",
        ].includes(key.toLowerCase())
    )
  )
  const response = await fetch(task.url, {
    method: "PUT",
    body: file,
    headers,
    signal,
    redirect: "error",
    credentials: "omit",
  })
  if (!response.ok)
    throw Error(`文件上传失败（HTTP ${response.status}），可重试同一任务`)
}
