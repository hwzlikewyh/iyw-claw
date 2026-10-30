"use client"

import type { ReactNode } from "react"
import { usePathname } from "next/navigation"
import { ThemeProvider } from "@/components/theme-provider"
import { AppearanceProvider } from "@/components/appearance-provider"
import { TooltipProvider } from "@/components/ui/tooltip"
import { AppToaster } from "@/components/ui/app-toaster"

export function PublicShareBoundary({
  children,
  content,
}: {
  children: ReactNode
  content: ReactNode
}) {
  const path = usePathname()
  if (!["/share", "/share/", "/share.html"].includes(path)) return children
  // 公开快照不初始化工作台登录、WebSocket 或桌面生命周期。
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <AppearanceProvider>
        <TooltipProvider>
          {content}
          <AppToaster />
        </TooltipProvider>
      </AppearanceProvider>
    </ThemeProvider>
  )
}
