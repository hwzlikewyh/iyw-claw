import {
  getFolderConversation,
  listAllTaskArtifacts,
  readFileBase64,
} from "@/lib/api"
import type { AgentType, MessageTurn } from "@/lib/types"
import {
  fetchDisplayAsset,
  isDisplayAssetUri,
} from "@/lib/display-asset-source"
import type {
  ShareCandidate,
  ShareMessage,
  ShareFileSpec,
  ShareSnapshot,
} from "./types"

export const SHARE_MAX_FILE_BYTES = 128 * 1024 * 1024
const SHARE_MAX_TOTAL_BYTES = 512 * 1024 * 1024

export async function loadShareSource(
  conversationId: number,
  agentType: AgentType
) {
  let detail = await getFolderConversation(conversationId)
  const turns = [...detail.turns]
  while (detail.history_start > 0) {
    const before = detail.history_start
    detail = await getFolderConversation(conversationId, before)
    if (detail.history_start >= before)
      throw new Error("完整历史加载失败，请重试")
    turns.unshift(...detail.turns)
  }
  const seen = new Set<string>()
  const uniqueTurns = turns.filter((turn) => {
    if (seen.has(turn.id)) return false
    seen.add(turn.id)
    return true
  })
  const messages = uniqueTurns.flatMap(toShareMessage)
  const artifacts = await listAllTaskArtifacts({
    conversationId,
    latestTurnOnly: false,
  })
  for (const turn of uniqueTurns) {
    if (
      turn.role !== "assistant" ||
      messages.some((message) => message.id === turn.id)
    )
      continue
    if (artifacts.some((artifact) => artifact.messageId === turn.id)) {
      messages.push({
        id: turn.id,
        role: "assistant",
        timestamp: turn.timestamp,
        blocks: [{ type: "text", text: "已生成成果文件。" }],
      })
    }
  }
  const order = new Map(uniqueTurns.map((turn, index) => [turn.id, index]))
  messages.sort((left, right) => order.get(left.id)! - order.get(right.id)!)
  const ids = new Set(messages.map((message) => message.id))
  const candidates: ShareCandidate[] = artifacts.flatMap((item) =>
    (item.kind === "file" || item.kind === "url") &&
    item.status === "available" &&
    item.messageId &&
    ids.has(item.messageId)
      ? [
          {
            reference: `artifact_${item.id}`,
            messageId: item.messageId,
            name: item.displayName,
            ...(item.kind === "url" ? { uri: item.path } : { path: item.path }),
          },
        ]
      : []
  )
  collectImages(uniqueTurns, messages, candidates)
  if (!messages.length) throw new Error("当前会话没有可分享的消息")
  return {
    title: detail.summary.title || "会话分享",
    snapshot: { version: 1, agentType, messages } as ShareSnapshot,
    candidates,
  }
}

function toShareMessage(turn: MessageTurn): ShareMessage[] {
  if (turn.role !== "user" && turn.role !== "assistant") return []
  const blocks = turn.blocks.flatMap((block) =>
    block.type === "text"
      ? [{ type: "text" as const, text: cleanShareText(block.text) }]
      : []
  )
  const hasImage = turn.blocks.some((block) =>
    ["image", "image_generation", "display_image"].includes(block.type)
  )
  return blocks.length || hasImage
    ? [{ id: turn.id, role: turn.role, timestamp: turn.timestamp, blocks }]
    : []
}

function collectImages(
  turns: MessageTurn[],
  messages: ShareMessage[],
  candidates: ShareCandidate[]
) {
  const byID = new Map(messages.map((message) => [message.id, message]))
  for (const turn of turns) {
    const message = byID.get(turn.id)
    if (!message) continue
    turn.blocks.forEach((block, index) => {
      const image =
        block.type === "image"
          ? block
          : block.type === "image_generation" || block.type === "display_image"
            ? block.image
            : null
      if (!image?.data && !image?.uri) return
      const reference = `image_${candidates.length}_${index}`
      const extension =
        image.mime_type.split("/")[1]?.replace("jpeg", "jpg") || "png"
      candidates.push({
        reference,
        messageId: turn.id,
        name: `图片-${index + 1}.${extension}`,
        data: image.data || undefined,
        uri: image.uri || undefined,
        mimeType: image.mime_type,
      })
      message.blocks.push({ type: "file", reference })
    })
  }
}

export function cleanShareText(text: string) {
  return text
    .replace(/<!-- IYW_CLAW_AGENT_LESSON_V1[\s\S]*?(?:-->|$)/g, "")
    .replace(/!?\[([^\]]*)\]\((?:file:|iyw-claw:|[a-z]:[\\/])[^)]*\)/gi, "$1")
    .replace(/([a-z]:[\\/])(?:[^\s<>"|]+[\\/])+([^\s<>"|\\/]+)/gi, "$2")
}

export async function prepareShareFiles(candidates: ShareCandidate[]) {
  const specs: ShareFileSpec[] = []
  const blobs = new Map<string, Blob>()
  let total = 0
  for (const candidate of candidates) {
    const blob = await candidateBlob(candidate)
    total += blob.size
    if (
      !blob.size ||
      blob.size > SHARE_MAX_FILE_BYTES ||
      total > SHARE_MAX_TOTAL_BYTES
    ) {
      throw new Error(
        "单个成果需为 1 字节至 128 MiB，分享文件合计不超过 512 MiB"
      )
    }
    const digest = await crypto.subtle.digest(
      "SHA-256",
      await blob.arrayBuffer()
    )
    const sha256 = Array.from(new Uint8Array(digest), (byte) =>
      byte.toString(16).padStart(2, "0")
    ).join("")
    specs.push({
      reference: candidate.reference,
      messageId: candidate.messageId,
      fileName: candidate.name,
      contentType: blob.type || "application/octet-stream",
      sizeBytes: blob.size,
      sha256,
    })
    blobs.set(candidate.reference, blob)
  }
  return { specs, blobs }
}

async function candidateBlob(candidate: ShareCandidate): Promise<Blob> {
  if (candidate.file) return candidate.file
  if (isDisplayAssetUri(candidate.uri))
    return fetchDisplayAsset(candidate.uri!, candidate.mimeType)
  if (candidate.uri && /^https?:\/\//i.test(candidate.uri)) {
    const response = await fetch(candidate.uri, {
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal: AbortSignal.timeout(60_000),
    })
    if (!response.ok)
      throw new Error(`无法读取成果“${candidate.name}”，请下载后手动上传`)
    if (Number(response.headers.get("Content-Length")) > SHARE_MAX_FILE_BYTES)
      throw new Error("成果超过 128 MiB")
    return response.blob()
  }
  const encoded =
    candidate.data ??
    (await readFileBase64(candidate.path!, SHARE_MAX_FILE_BYTES))
  const binary = atob(encoded.replace(/^data:[^,]+,/, ""))
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0))
  return new Blob([bytes], {
    type: candidate.mimeType || mimeType(candidate.name),
  })
}

function mimeType(name: string) {
  const extension = name.split(".").pop()?.toLowerCase() ?? ""
  const types: Record<string, string> = {
    pdf: "application/pdf",
    md: "text/markdown",
    txt: "text/plain",
    csv: "text/csv",
    json: "application/json",
    html: "text/html",
    png: "image/png",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    webp: "image/webp",
    gif: "image/gif",
    svg: "image/svg+xml",
    mp4: "video/mp4",
    webm: "video/webm",
    mp3: "audio/mpeg",
    wav: "audio/wav",
  }
  return types[extension] || "application/octet-stream"
}
