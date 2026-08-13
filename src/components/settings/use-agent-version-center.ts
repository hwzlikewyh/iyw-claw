"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  agentVersionCenterSnapshot,
  getAgentVersionHistory,
  installAgentVersion,
  refreshAgentVersionCenter,
  rollbackAgentVersion,
  setAgentVersionPin,
  switchAgentVersion,
} from "@/lib/api"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { toErrorMessage } from "@/lib/app-error"
import type {
  AgentType,
  AgentVersionCenterSnapshot,
  AgentVersionHistory,
  AgentVersionInventory,
} from "@/lib/types"

type Operation = "install" | "switch" | "pin" | "rollback" | "refresh"
type CenterData = {
  snapshot: AgentVersionCenterSnapshot
  history: AgentVersionHistory
}

async function fetchCenterData(
  agentType: AgentType,
  refreshCatalog: boolean
): Promise<CenterData> {
  const snapshot = refreshCatalog
    ? await refreshAgentVersionCenter()
    : await agentVersionCenterSnapshot()
  const inventory = snapshot.agents.find((item) => item.agentType === agentType)
  const history = await getAgentVersionHistory(
    agentType,
    inventory?.updateChannel
  )
  return { snapshot, history }
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
  const selectedVersion = versions.some(
    (item) => item.version === requestedVersion
  )
    ? requestedVersion
    : recommended || inventory?.activeVersion || versions[0]?.version || ""
  return { selectedVersion, selectVersion }
}

function useVersionOperations(args: {
  agentType: AgentType
  selectedVersion: string
  inventory?: AgentVersionInventory
  isPinned: boolean
  load: (refresh: boolean) => Promise<void>
  setError: (error: string | null) => void
  onChanged: () => Promise<void>
}) {
  const { agentType, selectedVersion, load, setError, onChanged } = args
  const t = useTranslations("AcpAgentSettings.versionCenter")
  const [busy, setBusy] = useState<Operation | null>(null)
  const run = useCallback(
    async (operation: Operation, action: () => Promise<unknown>) => {
      setBusy(operation)
      setError(null)
      try {
        await action()
        await Promise.all([load(false), onChanged()])
        toast.success(
          t(`success.${operation}`, {
            name: getAgentDisplayName(agentType),
            version: selectedVersion,
          })
        )
      } catch (reason) {
        const message = toErrorMessage(reason)
        setError(message)
        toast.error(t("operationFailed"), { description: message })
      } finally {
        setBusy(null)
      }
    },
    [agentType, load, onChanged, selectedVersion, setError, t]
  )
  return buildOperations(args, busy, setBusy, run, t)
}

function buildOperations(
  args: Parameters<typeof useVersionOperations>[0],
  busy: Operation | null,
  setBusy: (value: Operation | null) => void,
  run: (operation: Operation, action: () => Promise<unknown>) => Promise<void>,
  t: ReturnType<typeof useTranslations>
) {
  const { agentType, selectedVersion, inventory, isPinned, load } = args
  const refresh = () => {
    setBusy("refresh")
    void Promise.all([load(true), args.onChanged()])
      .then(() => toast.success(t("success.refresh")))
      .catch(() => {})
      .finally(() => setBusy(null))
  }
  return {
    busy,
    refresh,
    install: () =>
      void run("install", () =>
        installAgentVersion(agentType, selectedVersion)
      ),
    switchVersion: () =>
      void run("switch", () => switchAgentVersion(agentType, selectedVersion)),
    togglePin: () =>
      void run("pin", () =>
        setAgentVersionPin(
          agentType,
          isPinned ? null : selectedVersion,
          inventory?.updateChannel
        )
      ),
    rollback: () => void run("rollback", () => rollbackAgentVersion(agentType)),
  }
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
    accessDenied:
      args.catalogStale ||
      !args.platform ||
      !["active", "hidden"].includes(args.platform.status),
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
