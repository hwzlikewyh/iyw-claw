import localforage from "localforage"
import type { App } from "@modelcontextprotocol/ext-apps"
import { AssetClient, type AssetReference } from "../asset-client.js"

type CanvasHost = Pick<App, "callServerTool">
type StoredEntry = { key: string; value: unknown }
type StoreContext = { name: string; storeName: string; cache: Map<string, unknown> }
type DriverThis = { _config: { name?: string; storeName?: string; description?: string; version?: number }; _dbInfo?: StoreContext }

const assetMarker = "__iywInfiniteCanvasAsset"
const contexts = new Map<string, StoreContext>()

export async function getStoredAssetReferences(app: CanvasHost, storageName: string): Promise<Map<string, AssetReference>> {
  const namespace = `infinite-canvas:${storageName}`
  const result = new Map<string, AssetReference>()
  for (const [key, value] of await readNamespace(app, namespace)) {
    if (!value || typeof value !== "object") continue
    const reference = (value as Record<string, unknown>)[assetMarker]
    if (reference && typeof reference === "object") result.set(key, reference as AssetReference)
  }
  return result
}

export async function installPluginStorage(app: CanvasHost, assets: AssetClient): Promise<void> {
  await localforage.defineDriver(createStorageDriver(app, assets))
  await localforage.setDriver("iyw-infinite-canvas")
  const createInstance = localforage.createInstance.bind(localforage)
  localforage.createInstance = (options) => createInstance({ ...options, driver: "iyw-infinite-canvas" })
  const values = await readNamespace(app, "browser-local-storage")
  installLocalStorage(app, values)
}

function createStorageDriver(app: CanvasHost, assets: AssetClient) {
  return {
    _driver: "iyw-infinite-canvas",
    _support: async () => true,
    async _initStorage(this: DriverThis): Promise<void> {
      const name = this._config.name || "infinite-canvas"
      const storeName = this._config.storeName || "keyvaluepairs"
      const namespace = `${name}:${storeName}`
      const cache = new Map(await readNamespace(app, namespace))
      const context = { name: namespace, storeName, cache }
      contexts.set(namespace, context)
      this._dbInfo = context
    },
    async clear(this: DriverThis): Promise<void> {
      const context = requireContext(this)
      for (const key of context.cache.keys()) await writeEntry(app, context.name, key, "delete")
      context.cache.clear()
    },
    async getItem<T>(this: DriverThis, key: string): Promise<T | null> {
      const value = requireContext(this).cache.get(key)
      return value === undefined ? null : await decodeValue(value, assets) as T
    },
    async iterate<T, U>(this: DriverThis, iteratee: (value: T, key: string, iterationNumber: number) => U): Promise<U> {
      const cache = requireContext(this).cache
      let index = 0
      for (const [key, value] of cache) {
        const result = iteratee(await decodeValue(value, assets) as T, key, ++index)
        if (result !== undefined) return result
      }
      return undefined as U
    },
    async key(this: DriverThis, index: number): Promise<string> {
      return ([...requireContext(this).cache.keys()][index] ?? null) as unknown as string
    },
    async keys(this: DriverThis): Promise<string[]> {
      return [...requireContext(this).cache.keys()]
    },
    async length(this: DriverThis): Promise<number> {
      return requireContext(this).cache.size
    },
    async removeItem(this: DriverThis, key: string): Promise<void> {
      const context = requireContext(this)
      await writeEntry(app, context.name, key, "delete")
      context.cache.delete(key)
    },
    async setItem<T>(this: DriverThis, key: string, value: T): Promise<T> {
      const context = requireContext(this)
      const stored = await encodeValue(value, assets)
      await writeEntry(app, context.name, key, "set", stored)
      context.cache.set(key, stored)
      return value
    },
  }
}

function requireContext(driver: DriverThis): StoreContext {
  if (driver._dbInfo) return driver._dbInfo
  const namespace = `${driver._config.name || "infinite-canvas"}:${driver._config.storeName || "keyvaluepairs"}`
  const context = contexts.get(namespace)
  if (!context) throw new Error("Infinite Canvas storage is not initialized")
  return context
}

async function readNamespace(app: CanvasHost, namespace: string): Promise<Array<[string, unknown]>> {
  const data = await callStore(app, { operation: "list", namespace }) as { entries?: StoredEntry[] }
  return (data.entries || []).filter((entry) => typeof entry.key === "string").map((entry) => [entry.key, entry.value])
}

async function writeEntry(app: CanvasHost, namespace: string, key: string, operation: "set" | "delete", value?: unknown): Promise<void> {
  await callStore(app, { operation, namespace, key, ...(operation === "set" ? { value } : {}) })
}

async function callStore(app: CanvasHost, arguments_: Record<string, unknown>): Promise<unknown> {
  const result = await app.callServerTool({ name: "infinite_canvas_store_data", arguments: arguments_ })
  const text = result.content.find((part) => part.type === "text")?.text
  if (result.isError || !text) throw new Error(text || "Workspace storage is unavailable")
  try {
    return JSON.parse(text)
  } catch {
    throw new Error("Workspace storage returned invalid data")
  }
}

async function encodeValue(value: unknown, assets: AssetClient): Promise<unknown> {
  if (value instanceof Blob) {
    const reference = await assets.upload(value, "canvas-store-asset", value.type || "application/octet-stream")
    return { [assetMarker]: reference }
  }
  if (Array.isArray(value)) return Promise.all(value.map((item) => encodeValue(item, assets)))
  if (value && typeof value === "object") {
    return Object.fromEntries(await Promise.all(Object.entries(value).map(async ([key, item]) => [key, await encodeValue(item, assets)])))
  }
  return value
}

async function decodeValue(value: unknown, assets: AssetClient): Promise<unknown> {
  if (Array.isArray(value)) return Promise.all(value.map((item) => decodeValue(item, assets)))
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>
    const reference = record[assetMarker]
    if (reference && typeof reference === "object") return loadAsset(reference as AssetReference, assets)
    return Object.fromEntries(await Promise.all(Object.entries(record).map(async ([key, item]) => [key, await decodeValue(item, assets)])))
  }
  return value
}

async function loadAsset(reference: AssetReference, assets: AssetClient): Promise<Blob> {
  const url = await assets.getUrl(reference)
  return fetch(url).then((response) => response.blob())
}

function installLocalStorage(app: CanvasHost, initial: Array<[string, unknown]>): void {
  const values = new Map(initial.filter((entry): entry is [string, string] => typeof entry[1] === "string"))
  const storage: Storage = {
    get length() { return values.size },
    clear() {
      for (const key of values.keys()) void writeEntry(app, "browser-local-storage", key, "delete")
      values.clear()
    },
    getItem(key) { return values.get(key) ?? null },
    key(index) { return [...values.keys()][index] ?? null },
    removeItem(key) {
      values.delete(key)
      void writeEntry(app, "browser-local-storage", key, "delete")
    },
    setItem(key, value) {
      values.set(key, String(value))
      void writeEntry(app, "browser-local-storage", key, "set", String(value)).catch((error) => console.error("Canvas local preference save failed", error instanceof Error ? error.message : "unknown error"))
    },
  }
  Object.defineProperty(window, "localStorage", { configurable: true, value: storage })
}
