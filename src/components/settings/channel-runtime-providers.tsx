"use client"

import type { ReactNode } from "react"
import {
  AcpConnectionsProvider,
  useOptionalConnectionStore,
} from "@/contexts/acp-connections-context"
import { AlertProvider } from "@/contexts/alert-context"
import { DelegationProvider } from "@/contexts/delegation-context"
import { IywAccountProvider } from "@/contexts/iyw-account-context"
import { WorkspaceProvider } from "@/contexts/workspace-context"
import { InteractivePageProvider } from "@/contexts/interactive-page-context"

// 工作区设置复用现有上下文；独立设置页补齐消息渲染器需要的读取上下文。
export function ChannelRuntimeProviders({ children }: { children: ReactNode }) {
  const connections = useOptionalConnectionStore()
  if (connections) return children
  return (
    <IywAccountProvider>
      <AlertProvider>
        <AcpConnectionsProvider>
          <DelegationProvider>
            <WorkspaceProvider>
              <InteractivePageProvider>{children}</InteractivePageProvider>
            </WorkspaceProvider>
          </DelegationProvider>
        </AcpConnectionsProvider>
      </AlertProvider>
    </IywAccountProvider>
  )
}
