"use client"

import { useEffect, useRef, useState, type ReactNode } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import {
  useAcpActions,
  useConnectionStore,
} from "@/contexts/acp-connections-context"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { toErrorMessage } from "@/lib/app-error"
import { getShellTransport } from "@/lib/transport"
import {
  conversationWindowLabel,
  MOVE_CONVERSATION_WINDOW_EVENT,
} from "@/lib/conversation-window-state"
import {
  moveConversationWindow,
  restoreConversationWindow,
} from "@/lib/conversation-window-transfer"

function TransferLoading() {
  const t = useTranslations("ConversationTabs")
  return (
    <div
      role="status"
      aria-live="polite"
      className="fixed inset-0 z-[100] flex items-center justify-center gap-2 bg-background/90 text-sm text-muted-foreground"
    >
      <Loader2 className="size-4 animate-spin" />
      {t("moving")}
    </div>
  )
}

function useWindowInitialization() {
  const actions = useAcpActions()
  const connections = useConnectionStore()
  const foldersHydrated = useAppWorkspaceStore((state) => state.foldersHydrated)
  const [label] = useState(conversationWindowLabel)
  const [ready, setReady] = useState(!label)
  const initialization = useRef<Promise<void> | null>(null)

  useEffect(() => {
    if (!label || !foldersHydrated) return
    let active = true
    initialization.current ??= restoreConversationWindow(label, actions, connections)
    void initialization.current
      .then(() => {
        if (active) setReady(true)
      })
      .catch(async (error) => {
        console.error("[conversation-window] initialization failed", error)
        await getShellTransport()
          .call("finish_conversation_window", { label })
          .catch((cleanupError) =>
            console.error("[conversation-window] cleanup failed", cleanupError)
          )
      })
    return () => {
      active = false
    }
  }, [actions, connections, foldersHydrated, label])
  return ready
}

function useMoveWindow(ready: boolean) {
  const t = useTranslations("ConversationTabs")
  const actions = useAcpActions()
  const connections = useConnectionStore()
  const [moving, setMoving] = useState(false)
  const busy = useRef(false)
  useEffect(() => {
    const onMove = (event: Event) => {
      const tabId = (event as CustomEvent<string>).detail
      if (busy.current || !ready || !tabId) return
      busy.current = true
      setMoving(true)
      void moveConversationWindow({ tabId, actions, connections })
        .catch((error) => {
          console.error("[conversation-window] move failed", error)
          toast.error(t("moveFailed", { message: toErrorMessage(error) }))
        })
        .finally(() => {
          busy.current = false
          setMoving(false)
        })
    }
    window.addEventListener(MOVE_CONVERSATION_WINDOW_EVENT, onMove)
    return () =>
      window.removeEventListener(MOVE_CONVERSATION_WINDOW_EVENT, onMove)
  }, [actions, connections, ready, t])
  return moving
}

export function ConversationWindowGate({ children }: { children: ReactNode }) {
  const ready = useWindowInitialization()
  const moving = useMoveWindow(ready)
  useEffect(() => {
    if (!moving) return
    const blockKeyboard = (event: KeyboardEvent) => {
      event.preventDefault()
      event.stopImmediatePropagation()
    }
    window.addEventListener("keydown", blockKeyboard, true)
    return () => window.removeEventListener("keydown", blockKeyboard, true)
  }, [moving])
  if (!ready) return <TransferLoading />
  return (
    <>
      <div className="contents" inert={moving || undefined}>
        {children}
      </div>
      {moving && <TransferLoading />}
    </>
  )
}
