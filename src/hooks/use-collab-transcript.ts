"use client"

import { useEffect, useReducer } from "react"
import { getConversation } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { ConversationDetail } from "@/lib/types"

const REFRESH_MS = 3000
const MAX_FAILURES = 3
const SETTLED_READS = 3
const THREAD_ID = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i

interface TranscriptState {
  detail: ConversationDetail | null
  loading: boolean
  error: string | null
}

function reducer(state: TranscriptState, patch: Partial<TranscriptState>) {
  return { ...state, ...patch }
}

export function useCollabTranscript(threadId: string, active: boolean) {
  const [revision, refresh] = useReducer((value: number) => value + 1, 0)
  const [state, dispatch] = useReducer(reducer, {
    detail: null,
    loading: true,
    error: null,
  })
  useEffect(() => {
    if (!THREAD_ID.test(threadId)) {
      dispatch({ loading: false })
      return
    }
    dispatch({ loading: true })
    return subscribeTranscript({ threadId, active }, dispatch)
  }, [threadId, active, revision])
  return { ...state, refresh }
}

function subscribeTranscript(
  { threadId, active }: { threadId: string; active: boolean },
  dispatch: (patch: Partial<TranscriptState>) => void
) {
  let cancelled = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let failures = 0
  let reads = 0
  const load = async () => {
    try {
      const detail = await getConversation("codex", threadId)
      if (cancelled) return
      failures = 0
      dispatch({ detail, loading: false, error: null })
    } catch (error) {
      if (cancelled) return
      failures += 1
      dispatch({ loading: false, error: toErrorMessage(error) })
    }
    reads += 1
    if (
      !cancelled &&
      failures < MAX_FAILURES &&
      (active || reads < SETTLED_READS)
    ) {
      timer = setTimeout(load, REFRESH_MS)
    }
  }
  void load()
  return () => {
    cancelled = true
    clearTimeout(timer)
  }
}
