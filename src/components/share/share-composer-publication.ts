import { useCallback, useEffect, useRef, useState } from "react"
import { prepareShareFiles } from "@/lib/conversation-share/prepare"
import { publishShare } from "@/lib/conversation-share/publish"
import { toErrorMessage } from "@/lib/app-error"
import type {
  ShareCandidate,
  ShareSnapshot,
} from "@/lib/conversation-share/types"

type Prepared = Awaited<ReturnType<typeof prepareShareFiles>> & {
  urls: Map<string, string>
}
interface PublicationInput {
  conversationId: number
  snapshot: ShareSnapshot | null
  files: ShareCandidate[]
}

export function useSharePublication(input: PublicationInput) {
  const [title, setTitle] = useState<string | null>(null)
  const [days, setDays] = useState(7)
  const [error, setError] = useState("")
  const [status, setStatus] = useState("")
  const [preview, setPreview] = useState<Prepared | null>(null)
  const [result, setResult] = useState<Awaited<
    ReturnType<typeof publishShare>
  > | null>(null)
  const pending = useRef(false)
  const revision = useRef(0)
  const reset = useCallback(() => {
    revision.current += 1
    pending.current = false
    setTitle(null)
    setDays(7)
    setError("")
    setStatus("")
    setPreview(null)
    setResult(null)
  }, [])
  useEffect(
    () => () => {
      revision.current += 1
    },
    []
  )
  useEffect(
    () => () => preview?.urls.forEach((url) => URL.revokeObjectURL(url)),
    [preview]
  )
  const run = async (work: (current: () => boolean) => Promise<void>) => {
    if (pending.current || result) return
    pending.current = true
    const version = revision.current
    const current = () => version === revision.current
    setStatus("正在准备成果…")
    setError("")
    try {
      await work(current)
    } catch (err) {
      if (current()) setError(toErrorMessage(err))
    } finally {
      if (current()) {
        pending.current = false
        setStatus("")
      }
    }
  }
  return {
    title,
    setTitle,
    days,
    setDays,
    error,
    status,
    preview,
    result,
    previewFiles: preview?.urls ?? new Map<string, string>(),
    reset,
    back: () => {
      if (!pending.current && !result) {
        setPreview(null)
        setError("")
      }
    },
    prepare: () =>
      run(async (current) => {
        const prepared = await prepareShareFiles(input.files)
        if (!current()) return
        const urls = new Map(
          Array.from(prepared.blobs, ([key, blob]) => [
            key,
            URL.createObjectURL(blob),
          ])
        )
        setPreview({ ...prepared, urls })
      }),
    publish: (fallbackTitle: string) =>
      run(async (current) => {
        if (!preview || !input.snapshot?.messages.length) return
        const published = await publishShare(
          {
            title: title ?? fallbackTitle,
            sourceConversationId: String(input.conversationId),
            expirationDays: days,
            snapshot: input.snapshot,
            files: preview.specs,
          },
          preview.blobs,
          (value) => {
            if (current()) setStatus(value)
          }
        )
        if (current()) setResult(published)
      }),
  }
}
