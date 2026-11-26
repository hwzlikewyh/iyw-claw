"use client"

import { useEffect, useSyncExternalStore, type ReactNode } from "react"
import { Loader2 } from "lucide-react"
import { APP_LOGO, APP_NAME } from "@/lib/brand"

const subscribe = () => () => undefined
const clientReady = () => true
const serverReady = () => false

export function BrandBootstrap({ children }: { children: ReactNode }) {
  const ready = useSyncExternalStore(subscribe, clientReady, serverReady)
  useEffect(() => {
    const update = () => {
      if (!document.title.includes(APP_NAME)) {
        document.title = document.title.replaceAll("原助理", APP_NAME)
      }
      if (APP_LOGO !== "/icon.svg") {
        document
          .querySelectorAll<HTMLLinkElement>(
            'link[rel="icon"],link[rel="apple-touch-icon"]'
          )
          .forEach((link) => {
            if (link.href !== APP_LOGO) link.href = APP_LOGO
          })
      }
    }
    update()
    const observer = new MutationObserver(update)
    observer.observe(document.head, {
      childList: true,
      characterData: true,
      subtree: true,
    })
    return () => observer.disconnect()
  }, [])
  if (!ready) {
    return (
      <div
        className="flex min-h-screen items-center justify-center bg-background"
        aria-busy="true"
      >
        <Loader2 className="size-4 animate-spin" aria-label="正在启动" />
      </div>
    )
  }
  return children
}
