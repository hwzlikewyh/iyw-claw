"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"

import {
  acpListAgents,
  agentVersionCenterSnapshot,
  getAgentVersionHistory,
  refreshAgentVersionCenter,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type {
  AgentType,
  AgentVersionCenterSnapshot,
  AgentVersionHistory,
  AgentVersionInventory,
} from "@/lib/types"

import { mergeVersionHistory } from "./agent-version-candidates"
import { useVersionOperations } from "./use-agent-version-operations"

type CenterData = {
  snapshot: AgentVersionCenterSnapshot
  history: AgentVersionHistory
  historyError: string | null
}

async function fetchCenterData(
  agentType: AgentType,
  refreshCatalog: boolean
): Promise<CenterData> {
  const snapshot = refreshCatalog
    ? await refreshAgentVersionCenter()
    : await agentVersionCenterSnapshot()
  const inventory = snapshot.agents.find((item) => item.agentType === agentType)
  const [registryAgents, historyResult] = await Promise.all([
    acpListAgents().catch(() => []),
    getAgentVersionHistory(agentType, inventory?.updateChannel)
      .then((history) => ({ history, error: null }))
      .catch((reason) => ({
        history: { items: [] },
        error: toErrorMessage(reason),
      })),
  ])
  const registryVersion = registryAgents.find(
    (agent) => agent.agent_type === agentType
  )?.registry_version
  return {
    snapshot,
    history: mergeVersionHistory(
      historyResult.history,
      agentType,
      inventory,
      registryVersion
    ),
    historyError: historyResult.error,
  }
}

function useCenterData(agentType: AgentType) {
  const generation = useRef(0)
  const [data, setData] = useState<CenterData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const load = useCallback(
    async (refreshCatalog: boolean) => {
      const request = generation.current + 1
      generation.current = request
      try {
        const next = await fetchCenterData(agentType, refreshCatalog)
        if (request === generation.current) {
          setData(next)
          setError(null)
        }
      } catch (reason) {
        if (request === generation.current) setError(toErrorMessage(reason))
        throw reason
      }
    },
    [agentType]
  )
  useEffect(() => {
    const request = generation.current + 1
    generation.current = request
    void fetchCenterData(agentType, false)
      .then((next) => {
        if (request !== generation.current) return
        setData(next)
        setError(null)
      })
      .catch((reason) => {
        if (request === generation.current) setError(toErrorMessage(reason))
      })
    return () => {
      generation.current += 1
    }
  }, [agentType])
  return { data, error, setError, load }
}

function useVersionSelection(
  versions: AgentVersionHistory["items"],
  recommended: string | null,
  inventory?: AgentVersionInventory
) {
  const [requestedVersion, selectVersion] = useState("")
  const recommendedVersion = versions.some(
    (item) => item.version === recommended
  )
    ? recommended
    : null
  const activeVersion = versions.some(
    (item) => item.version === inventory?.activeVersion
  )
    ? inventory?.activeVersion
    : null
  const selectedVersion = versions.some(
    (item) => item.version === requestedVersion
  )
    ? requestedVersion
    : recommendedVersion || activeVersion || versions[0]?.version || ""
  return { selectedVersion, selectVersion }
}

export function useAgentVersionCenter({
  agentType,
  onChanged,
}: {
  agentType: AgentType
  onChanged: () => Promise<void>
}) {
  const { data, error, setError, load } = useCenterData(agentType)
  const inventory = data?.snapshot.agents.find(
    (item) => item.agentType === agentType
  )
  const platform = data?.snapshot.catalog.snapshot.platforms.find(
    (item) => item.registryId === inventory?.registryId
  )
  const versions = useMemo(
    () => data?.history.items ?? [],
    [data?.history.items]
  )
  const recommended = platform?.recommendedVersion || null
  const { selectedVersion, selectVersion } = useVersionSelection(
    versions,
    recommended,
    inventory
  )
  const installedVersions = useMemo(
    () =>
      new Set(
        (inventory?.installations ?? [])
          .filter((item) => item.verified)
          .map((item) => item.version)
      ),
    [inventory?.installations]
  )
  const isPinned = inventory?.pinnedVersion === selectedVersion
  const operations = useVersionOperations({
    agentType,
    selectedVersion,
    inventory,
    isPinned,
    load,
    setError,
    onChanged,
  })
  const catalogStale = data?.snapshot.catalog.stale ?? true
  const historyError = data?.historyError ?? null
  return buildCenterState({
    data,
    error,
    inventory,
    platform,
    versions,
    recommended,
    selectedVersion,
    selectVersion,
    installedVersions,
    isPinned,
    catalogStale,
    historyError,
    operations,
  })
}

function buildCenterState(args: {
  data: CenterData | null
  error: string | null
  inventory?: AgentVersionInventory
  platform?: AgentVersionCenterSnapshot["catalog"]["snapshot"]["platforms"][number]
  versions: AgentVersionHistory["items"]
  recommended: string | null
  selectedVersion: string
  selectVersion: (version: string) => void
  installedVersions: Set<string>
  isPinned: boolean
  catalogStale: boolean
  historyError: string | null
  operations: ReturnType<typeof useVersionOperations>
}) {
  const { inventory, versions, selectedVersion, installedVersions } = args
  return {
    ready: Boolean(args.data && inventory),
    error: args.error,
    inventory,
    selectedVersion,
    selectVersion: args.selectVersion,
    versions,
    recommended: args.recommended,
    catalogStale: args.catalogStale,
    historyError: args.historyError,
    accessDenied: args.platform?.status === "disabled",
    isInstalled: installedVersions.has(selectedVersion),
    isActive: inventory?.activeVersion === selectedVersion,
    isPinned: args.isPinned,
    canPin:
      versions.find((item) => item.version === selectedVersion)?.pinnable ===
      true,
    rollbackReady: rollbackAvailable(inventory, installedVersions),
    ...args.operations,
  }
}

function rollbackAvailable(
  inventory: AgentVersionInventory | undefined,
  installedVersions: Set<string>
): boolean {
  return Boolean(
    inventory?.lastKnownGoodVersion &&
    inventory.lastKnownGoodVersion !== inventory.activeVersion &&
    installedVersions.has(inventory.lastKnownGoodVersion)
  )
}
