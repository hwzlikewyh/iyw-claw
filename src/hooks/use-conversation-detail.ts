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
  hasRuntimeContent: boolean
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
        hasRuntimeContent:
          (session?.localTurns.length ?? 0) > 0 ||
          (session?.optimisticTurns.length ?? 0) > 0 ||
          (session?.backgroundTurns.length ?? 0) > 0 ||
          session?.liveMessage != null,
      }
    })
  )
}

function useDetailLoader(args: {
  enabled: boolean
  conversationId: number
  canLoadVirtual: boolean
  canLoadDetail: boolean
  detail: DbConversationDetail | null
  detailLoading: boolean
}): void {
  const { fetchDetail, refetchDetail } = useConversationRuntimeActions()
  useEffect(() => {
    if (!args.enabled || !args.canLoadDetail) return
    if (args.detail || args.detailLoading) return
    if (args.canLoadVirtual) refetchDetail(args.conversationId)
    else fetchDetail(args.conversationId)
  }, [args, fetchDetail, refetchDetail])
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
  // and consumers re-render only on a real detail transition. The lightweight
  // runtime flags also let a cold virtual session reload from its persisted id.
  const snapshot = useDetailSnapshot(conversationId)
  const { detail, detailLoading, detailError, acpLoadError } = snapshot
  const isVirtual = isVirtualConversationId(conversationId)
  const canLoadVirtual =
    isVirtual &&
    snapshot.dbConversationId != null &&
    !snapshot.hasRuntimeContent
  const canLoadDetail = !isVirtual || canLoadVirtual
  useDetailLoader({
    enabled,
    conversationId,
    canLoadVirtual,
    canLoadDetail,
    detail,
    detailLoading,
  })

  return {
    detail,
    loading:
      detailLoading ||
      (enabled && canLoadDetail && detail == null && detailError == null),
    error: detailError,
    acpLoadError,
  }
}
