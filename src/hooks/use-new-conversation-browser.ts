"use client"

import { useEffect } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { onNewConversationOpened } from "@/stores/tab-store"

export function useNewConversationBrowser() {
  const { closeBrowser } = useBrowser()
  const { setOpen } = useAuxPanelContext()
  useEffect(() => {
    // 按新建成功事件收起面板，不再随草稿变化或浏览器轮询自动展开。
    return onNewConversationOpened((tabId) => {
      setOpen(false)
      void closeBrowser()
      console.info("[Browser] collapsed right panels for new conversation", {
        tabId,
      })
    })
  }, [closeBrowser, setOpen])
}
