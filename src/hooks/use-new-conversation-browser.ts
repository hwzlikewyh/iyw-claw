"use client"

import { useEffect, useRef } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { useTabStore } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { NATIVE_SHARE_OPENED } from "@/lib/native-share"

export function useNewConversationBrowser() {
  const draftId = useTabStore((store) => {
    const tab = store.tabs.find((item) => item.id === store.activeTabId)
    return tab?.conversationId === null ? tab.id : null
  })
  const { closeBrowser } = useBrowser()
  const { setOpen, restored } = useAuxPanelContext()
  const { isConversations } = useWorkbenchRoute()
  const initialized = useRef(new Set<string>())
  useEffect(() => {
    const close = () => {
      setOpen(false)
      void closeBrowser()
    }
    window.addEventListener(NATIVE_SHARE_OPENED, close)
    return () => window.removeEventListener(NATIVE_SHARE_OPENED, close)
  }, [closeBrowser, setOpen])
  useEffect(() => {
    if (
      !draftId ||
      !isConversations ||
      !restored ||
      initialized.current.has(draftId)
    )
      return
    // 新草稿默认收起右侧内容；之后只响应用户操作，不受浏览器轮询影响。
    initialized.current.add(draftId)
    setOpen(false)
    void closeBrowser()
  }, [closeBrowser, draftId, isConversations, restored, setOpen])
}
