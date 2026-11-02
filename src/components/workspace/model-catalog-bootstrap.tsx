"use client"

import { useEffect } from "react"
import { useIywAccount } from "@/contexts/iyw-account-context"
import { useSortedAvailableAgents } from "@/hooks/use-sorted-available-agents"
import { getGatewayModels } from "@/lib/gateway-model-catalog"

export function ModelCatalogBootstrap() {
  const { status, profile } = useIywAccount()
  if (status !== "authenticated" || profile?.workspace_pending) return null
  return <AuthenticatedModelCatalogBootstrap />
}

function AuthenticatedModelCatalogBootstrap() {
  const { sortedTypes, fresh } = useSortedAvailableAgents()

  useEffect(() => {
    console.info("[ModelCatalog] Starting workspace catalog preload")
    // 全量目录与默认星河目录并行，不等待环境安装或 Agent 列表扫描。
    void getGatewayModels()
    void getGatewayModels("codex")
  }, [])

  useEffect(() => {
    // 首次安装的 SDK 映射由 Agent 目录提供；就绪后重试未加载的目录。
    for (const agentType of sortedTypes) void getGatewayModels(agentType)
  }, [sortedTypes, fresh])

  return null
}
