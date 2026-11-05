"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useTabStore } from "@/stores/tab-store"
import { getShellTransport, isDesktop } from "@/lib/transport"
import {
  listNativeShares,
  nativeShareStatus,
  prepareNativeShare,
  shareError,
  OPEN_NATIVE_SHARE,
  updateNativeShareCount,
  type ShareBatch,
  type SharePreview,
  type ShareStatus,
} from "@/lib/native-share"

export interface ShareInboxState {
  open: boolean
  items: ShareBatch[]
  selected: string | null
  preview: SharePreview | null
  targetTab: string
  status: ShareStatus | null
  busy: boolean
  loading: boolean
  previewRevision: number
  error: string | null
}

type StateUpdate =
  | Partial<ShareInboxState>
  | ((previous: ShareInboxState) => Partial<ShareInboxState>)
export type PatchShareState = (update: StateUpdate) => void

export function useShareState() {
  const [state, setState] = useState<ShareInboxState>({
    open: false,
    items: [],
    selected: null,
    preview: null,
    targetTab: "",
    status: null,
    busy: false,
    loading: false,
    previewRevision: 0,
    error: null,
  })
  const patch = useCallback(
    (update: StateUpdate) =>
      setState((previous) => ({
        ...previous,
        ...(typeof update === "function" ? update(previous) : update),
      })),
    []
  )
  return { state, patch }
}

export function useShareRefresh(patch: PatchShareState) {
  const seen = useRef(new Set<string>())
  const generation = useRef(0)
  return useCallback(async () => {
    const request = ++generation.current
    const items = await listNativeShares()
    if (request !== generation.current) return
    const pending = items.filter((item) => !item.imported)
    updateNativeShareCount(pending.length)
    const fresh = pending.find((item) => !seen.current.has(item.id))
    items.forEach((item) => seen.current.add(item.id))
    patch((previous) => {
      const selected = previous.selected || fresh?.id || items[0]?.id || null
      const reveal = fresh && !previous.open && !previous.busy
      return {
        items,
        selected,
        ...(reveal
          ? {
              open: true,
              selected: fresh.id,
              loading: true,
              preview: null,
              targetTab: useTabStore.getState().activeTabId || "",
            }
          : {}),
      }
    })
  }, [patch])
}

export function useShareEvents(
  refresh: () => Promise<void>,
  patch: PatchShareState
) {
  useEffect(() => {
    if (!isDesktop()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    const reload = () => {
      void refresh().catch((error) => {
        if (!disposed) patch({ error: shareError(error) })
      })
    }
    const show = () => {
      patch({ open: true, targetTab: useTabStore.getState().activeTabId || "" })
      reload()
    }
    window.addEventListener(OPEN_NATIVE_SHARE, show)
    void getShellTransport()
      .subscribe("native-share://changed", reload)
      .then((off) => {
        if (disposed) off()
        else unlisten = off
      })
      .catch((error) => {
        if (!disposed) patch({ error: shareError(error) })
      })
    reload()
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener(OPEN_NATIVE_SHARE, show)
    }
  }, [refresh, patch])
}

export function useSharePreview(
  state: ShareInboxState,
  patch: PatchShareState
) {
  const { open, selected, previewRevision } = state
  useEffect(() => {
    if (!open || !selected) return
    let disposed = false
    void prepareNativeShare(selected)
      .then((preview) => {
        if (!disposed) patch({ preview, loading: false })
      })
      .catch((error) => {
        if (!disposed) patch({ error: shareError(error), loading: false })
      })
    return () => {
      disposed = true
    }
  }, [open, selected, previewRevision, patch])
}

export function useShareStatus(open: boolean, patch: PatchShareState) {
  useEffect(() => {
    if (!open || !isDesktop()) return
    let disposed = false
    void nativeShareStatus()
      .then((status) => {
        if (!disposed) patch({ status })
      })
      .catch((error) => {
        if (!disposed) patch({ error: shareError(error) })
      })
    return () => {
      disposed = true
    }
  }, [open, patch])
}
