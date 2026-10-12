"use client"

import type { ReactNode } from "react"
import {
  AcpConnectionsProvider,
  useOptionalConnectionStore,
} from "@/contexts/acp-connections-context"
import { AlertProvider } from "@/contexts/alert-context"
import { DelegationProvider } from "@/contexts/delegation-context"
import { IywAccountProvider } from "@/contexts/iyw-account-context"

// 工作区设置复用现有上下文；独立设置页补齐消息渲染器需要的读取上下文。
export function ChannelRuntimeProviders({ children }: { children: ReactNode }) {
  const connections = useOptionalConnectionStore()
  if (connections) return children
  return (
    <IywAccountProvider>
      <AlertProvider>
        <AcpConnectionsProvider>
          <DelegationProvider>{children}</DelegationProvider>
        </AcpConnectionsProvider>
      </AlertProvider>
    </IywAccountProvider>
  )
}
