"use client"

import { useEffect } from "react"
import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useBrowser } from "@/contexts/browser-context"
import { onNewConversationOpened } from "@/stores/tab-store"

export function useNewConversationBrowser() {
  const { closeBrowser } = useBrowser()
  const { setOpen } = useAuxPanelContext()
  useEffect(
    () =>
      onNewConversationOpened((tabId) => {
        // 新建只收起面板，保留浏览器标签及插件的未保存内容。
        setOpen(false)
        void closeBrowser()
        console.info("[Workspace] new conversation panels collapsed", { tabId })
      }),
    [closeBrowser, setOpen]
  )
}
