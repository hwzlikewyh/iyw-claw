"use client"

import type { ReactNode } from "react"
import { motion } from "motion/react"
import { PanelLeftOpen } from "lucide-react"
import { useTranslations } from "next-intl"
import { useTabStore } from "@/stores/tab-store"
import { AppTitleBar } from "@/components/layout/app-title-bar"
import { AppToaster } from "@/components/ui/app-toaster"
import { requestConversationWindow } from "@/lib/conversation-window-state"

function DetachedTitle() {
  const t = useTranslations("ConversationTabs")
  const tab = useTabStore((state) =>
    state.tabs.find((item) => item.id === state.activeTabId)
  )
  if (!tab) return null
  return (
    <motion.div
      drag
      dragSnapToOrigin
      dragMomentum={false}
      onDragStart={(event) => {
        if ("pointerId" in event && event.target instanceof Element) {
          try {
            event.target.setPointerCapture(event.pointerId)
          } catch (error) {
            console.warn(
              "[conversation-window] title drag capture failed",
              error
            )
          }
        }
      }}
      onDragEnd={(event) => {
        if (!event.type.includes("cancel"))
          requestConversationWindow(tab.id, {
            returnToWorkspace: true,
            dropOnly: true,
          })
      }}
      title={t("dragBackHint")}
      className="inline-flex min-w-0 max-w-[75%] cursor-grab items-center rounded px-2 py-1 text-xs active:cursor-grabbing"
    >
      <span className="truncate">{tab.title}</span>
    </motion.div>
  )
}

export function DetachedConversationShell({
  children,
}: {
  children: ReactNode
}) {
  const t = useTranslations("ConversationTabs")
  const activeId = useTabStore((state) => state.activeTabId)
  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-background text-foreground">
      <AppTitleBar
        left={
          <div data-tauri-drag-region className="flex min-w-0 items-center">
            <DetachedTitle />
            <div data-tauri-drag-region className="h-8 min-w-8 flex-1" />
          </div>
        }
        right={
          <button
            type="button"
            disabled={!activeId}
            onClick={() =>
              activeId &&
              requestConversationWindow(activeId, { returnToWorkspace: true })
            }
            className="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline focus-visible:outline-ring"
            title={t("returnToWorkspace")}
          >
            <PanelLeftOpen className="size-3.5" />
            {t("returnToWorkspace")}
          </button>
        }
      />
      <main className="min-h-0 flex-1">{children}</main>
      <AppToaster position="bottom-right" closeButton />
    </div>
  )
}
