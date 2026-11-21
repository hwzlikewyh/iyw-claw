"use client"

import { useEffect } from "react"
import { useShallow } from "zustand/react/shallow"
import {
  useConversationRuntimeActions,
  useConversationRuntimeStore,
} from "@/stores/conversation-runtime-store"
import type { DbConversationDetail } from "@/lib/types"

function isVirtualConversationId(conversationId: number): boolean {
  return !Number.isFinite(conversationId) || conversationId <= 0
}

interface DetailSnapshot {
  detail: DbConversationDetail | null
  detailLoading: boolean
  detailError: string | null
  acpLoadError: string | null
  dbConversationId: number | null
}

interface UseConversationDetailOptions {
  /** Disable when the caller owns fetching or the panel is not visible. */
  enabled?: boolean
}

interface ConversationDetailResult {
  detail: DbConversationDetail | null
  loading: boolean
  error: string | null
  acpLoadError: string | null
}

function useDetailSnapshot(conversationId: number): DetailSnapshot {
  return useConversationRuntimeStore(
    useShallow((state) => {
      const session = state.byConversationId.get(conversationId)
      return {
        detail: session?.detail ?? null,
        detailLoading: session?.detailLoading ?? false,
        detailError: session?.detailError ?? null,
        acpLoadError: session?.acpLoadError ?? null,
        dbConversationId: session?.dbConversationId ?? null,
      }
    })
  )
}

export function useConversationDetail(
  conversationId: number,
  options?: UseConversationDetailOptions
): ConversationDetailResult {
  const enabled = options?.enabled ?? true
  // Subscribe to ONLY the detail-related fields this hook exposes, not the whole
  // session object. The live-message sink replaces the session object on every
  // streaming batch (~60/s, via SET_LIVE_MESSAGE); a whole-session selector here
  // would re-render every consumer — notably the keep-alive conversation panel,
  // which calls this hook — on each streaming token. None of these fields change
  // mid-stream, so `useShallow` keeps the slice reference-stable across batches
  // and consumers re-render only on a detail transition or database id binding.
  const snapshot = useDetailSnapshot(conversationId)
  const { detail, detailLoading, detailError, acpLoadError } = snapshot
  const canLoadDetail = !isVirtualConversationId(
    snapshot.dbConversationId ?? conversationId
  )
  const { fetchDetail } = useConversationRuntimeActions()
  useEffect(() => {
    if (enabled && canLoadDetail && !detail && !detailLoading && !detailError) {
      fetchDetail(conversationId)
    }
  }, [
    enabled,
    canLoadDetail,
    detail,
    detailLoading,
    detailError,
    conversationId,
    fetchDetail,
  ])

  return {
    detail,
    loading:
      detailLoading ||
      (enabled && canLoadDetail && detail == null && detailError == null),
    error: detailError,
    acpLoadError,
  }
}
