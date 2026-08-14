import {
  parseGatewayModels,
  type GatewayModel,
} from "@/lib/gateway-model-parser"

export interface GatewayModelPayloadCache {
  read: () => unknown | null
  write: (payload: unknown) => void
}

interface GatewayModelCatalogOptions {
  fetchModels: () => Promise<unknown>
  cache: GatewayModelPayloadCache
  replaceWithEmpty?: boolean
}

export interface GatewayModelCatalog {
  getCached: () => GatewayModel[]
  hasAuthoritativeData: () => boolean
  load: () => Promise<GatewayModel[]>
  refresh: () => Promise<GatewayModel[]>
}

export function browserPayloadCache(key: string): GatewayModelPayloadCache {
  return {
    read: () => {
      try {
        const raw = globalThis.localStorage?.getItem(key)
        return raw ? JSON.parse(raw) : null
      } catch {
        return null
      }
    },
    write: (payload) => {
      try {
        globalThis.localStorage?.setItem(key, JSON.stringify(payload))
      } catch {
        // The in-memory online cache remains available for this app session.
      }
    },
  }
}

function modelPayloadData(payload: unknown): unknown[] | null {
  if (!payload || typeof payload !== "object") return null
  const data = (payload as { data?: unknown }).data
  return Array.isArray(data) ? data : null
}

export function createGatewayModelCatalog({
  fetchModels,
  cache,
  replaceWithEmpty = false,
}: GatewayModelCatalogOptions): GatewayModelCatalog {
  const persistedPayload = cache.read()
  const persistedData = modelPayloadData(persistedPayload)
  let cached = parseGatewayModels(persistedPayload)
  let authoritative =
    persistedData !== null && (persistedData.length === 0 || cached.length > 0)
  let loaded = false
  let pending: Promise<GatewayModel[]> | null = null

  const refresh = (): Promise<GatewayModel[]> => {
    if (pending) return pending
    pending = fetchModels()
      .then((payload) => {
        const data = modelPayloadData(payload)
        if (!data) return [...cached]
        const online = parseGatewayModels(payload)
        if (data.length > 0 && online.length === 0) return [...cached]
        if (online.length === 0 && !replaceWithEmpty) return [...cached]
        cached = online
        authoritative = true
        cache.write(payload)
        return [...cached]
      })
      .catch(() => [...cached])
      .finally(() => {
        loaded = true
        pending = null
      })
    return pending
  }

  return {
    getCached: () => [...cached],
    hasAuthoritativeData: () => authoritative,
    load: () => (loaded ? Promise.resolve([...cached]) : refresh()),
    refresh,
  }
}
