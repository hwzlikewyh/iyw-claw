import { shareAction, sharePageURL, validateShareBase } from "./client"
import type {
  ShareCreated,
  ShareFileSpec,
  ShareRecord,
  ShareSnapshot,
} from "./types"

interface PublishInput {
  title: string
  sourceConversationId: string
  expirationDays: number
  snapshot: ShareSnapshot
  files: ShareFileSpec[]
}

async function shareConfiguration() {
  const config = await shareAction<{ apiBase: string; webBase: string | null }>(
    "configuration",
    {}
  )
  const apiBase = validateShareBase(config.apiBase)
  const webBase =
    config.webBase?.trim() || `${apiBase}/conversation-shares/view`
  try {
    sharePageURL({ token: "0".repeat(64), apiBase }, webBase)
  } catch {
    throw new Error("分享网页地址无效，请配置已部署分享页的 HTTPS 地址。")
  }
  return { apiBase, webBase }
}

export async function publishShare(
  input: PublishInput,
  blobs: Map<string, Blob>,
  progress: (text: string) => void
) {
  const { apiBase, webBase } = await shareConfiguration()
  progress("正在创建分享…")
  const created = await shareAction<ShareCreated>("create", input)
  for (const [index, upload] of created.uploads.entries()) {
    const blob = blobs.get(upload.reference)
    if (!blob) throw new Error("成果内容缺失，请返回重新预览")
    progress(`正在上传成果 ${index + 1}/${created.uploads.length}…`)
    const response = await fetch(upload.url, {
      method: "PUT",
      headers: upload.headers,
      body: blob,
      signal: AbortSignal.timeout(120_000),
      referrerPolicy: "no-referrer",
    })
    if (!response.ok) throw new Error("成果上传失败，请检查网络并重试")
  }
  progress("正在发布并校验成果…")
  const record = await shareAction<ShareRecord>("publish", {
    id: created.share.id,
  })
  return {
    record,
    url: sharePageURL({ token: created.token, apiBase }, webBase),
  }
}
