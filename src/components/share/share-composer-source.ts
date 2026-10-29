import { useCallback, useEffect, useState } from "react"
import { loadShareSource } from "@/lib/conversation-share/prepare"
import type { AgentType } from "@/lib/types"
import type {
  ShareCandidate,
  ShareSnapshot,
} from "@/lib/conversation-share/types"
import { toErrorMessage } from "@/lib/app-error"

export type ShareSource = Awaited<ReturnType<typeof loadShareSource>>

export function useShareSource(
  conversationId: number,
  agentType: AgentType,
  enabled: boolean
) {
  const [source, setSource] = useState<ShareSource | null>(null)
  const [ids, setIDs] = useState(new Set<string>())
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  const reset = useCallback(() => {
    setSource(null)
    setIDs(new Set())
    setError("")
  }, [])
  useEffect(() => {
    if (!enabled) return
    let current = true
    loadShareSource(conversationId, agentType)
      .then((loaded) => {
        if (!current) return
        setSource(loaded)
        setIDs(new Set(loaded.snapshot.messages.map((message) => message.id)))
      })
      .catch((err) => {
        if (current) setError(toErrorMessage(err))
      })
    return () => {
      current = false
    }
  }, [conversationId, agentType, revision, enabled])
  return {
    source,
    setSource,
    ids,
    setIDs,
    loadError: error,
    reset,
    retry: () => {
      setError("")
      setRevision((value) => value + 1)
    },
  }
}

export function toggleSelection(values: Set<string>, value: string) {
  const next = new Set(values)
  if (next.has(value)) next.delete(value)
  else next.add(value)
  return next
}

export function selectedSnapshot(
  source: ShareSource | null,
  ids: Set<string>,
  files: ShareCandidate[]
): ShareSnapshot | null {
  if (!source) return null
  const included = new Set(files.map((file) => file.reference))
  const messages = source.snapshot.messages
    .filter((message) => ids.has(message.id))
    .map((message) => {
      const blocks = message.blocks.filter(
        (block) => block.type === "text" || included.has(block.reference!)
      )
      for (const file of files.filter(
        (file) => file.messageId === message.id
      )) {
        if (!blocks.some((block) => block.reference === file.reference))
          blocks.push({ type: "file", reference: file.reference })
      }
      if (!blocks.length)
        blocks.push({ type: "text", text: "此消息的附件未包含在分享中。" })
      return { ...message, blocks }
    })
  return { ...source.snapshot, messages }
}

export function appendFiles(input: {
  files: FileList
  source: ShareSource
  messageID: string
}) {
  const additions: ShareCandidate[] = Array.from(input.files, (file) => ({
    reference: `upload_${crypto.randomUUID()}`,
    messageId: input.messageID,
    name: file.name,
    file,
  }))
  return {
    source: {
      ...input.source,
      candidates: [...input.source.candidates, ...additions],
    },
    additions,
  }
}
