import {
  parseGatewayModels,
  type GatewayModel,
} from "@/lib/gateway-model-parser"
import { toErrorMessage } from "@/lib/app-error"

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

function onlineModels(payload: unknown, replaceWithEmpty: boolean) {
  const data = modelPayloadData(payload)
  if (!data) throw new Error("Gateway model response has no model array")
  const models = parseGatewayModels(payload)
  if (data.length > 0 && models.length === 0) {
    throw new Error("Gateway model response has no valid models")
  }
  if (models.length === 0 && !replaceWithEmpty) {
    throw new Error("Gateway model catalog is empty")
  }
  return models
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
        cached = onlineModels(payload, replaceWithEmpty)
        authoritative = true
        cache.write(payload)
        loaded = true
        console.info("[ModelCatalog] Refreshed models:", cached.length)
        return [...cached]
      })
      .catch((error) => {
        loaded = false
        console.warn("[ModelCatalog] Refresh failed; next load will retry", {
          error: toErrorMessage(error),
          cachedModelCount: cached.length,
          scoped: replaceWithEmpty,
        })
        return [...cached]
      })
      .finally(() => {
        pending = null
      })
    return pending
  }

  return {
    getCached: () => [...cached],
    hasAuthoritativeData: () => authoritative,
    load: async () => {
      // 等待早于登录或 SDK 目录就绪的请求结束，失败后重新拉取。
      if (pending) await pending
      return loaded ? [...cached] : refresh()
    },
    refresh,
  }
}
