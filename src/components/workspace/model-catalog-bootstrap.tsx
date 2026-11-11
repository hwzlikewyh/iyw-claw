"use client"

import { useEffect } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useSortedAvailableAgents } from "@/hooks/use-sorted-available-agents"
import { loadFixedAgentOptions } from "@/lib/fixed-agent-options"
import {
  getGatewayModels,
  refreshGatewayModels,
} from "@/lib/gateway-model-catalog"

export function ModelCatalogBootstrap() {
  const { status, profile } = useIywAccount()
  if (status !== "authenticated" || profile?.workspace_pending) return null
  return (
    <AuthenticatedModelCatalogBootstrap
      key={JSON.stringify([profile?.user_id, profile?.org_code])}
    />
  )
}

function AuthenticatedModelCatalogBootstrap() {
  const { sortedTypes, fresh, refresh } = useSortedAvailableAgents()

  useEffect(() => {
    console.info("[ModelCatalog] Starting workspace catalog preload")
    // 全量目录与默认星河目录并行，不等待环境安装或 Agent 列表扫描。
    void refreshGatewayModels().then(() => getGatewayModels())
    void loadFixedAgentOptions("codex")
    // 登录前的 Agent 扫描可能失败或为空，登录后立即重新确认可用列表。
    void refresh()
  }, [refresh])

  useEffect(() => {
    // 首次安装的 SDK 映射由 Agent 目录提供；就绪后重试未加载的目录。
    for (const agentType of sortedTypes) void loadFixedAgentOptions(agentType)
  }, [sortedTypes, fresh])

  return null
}
