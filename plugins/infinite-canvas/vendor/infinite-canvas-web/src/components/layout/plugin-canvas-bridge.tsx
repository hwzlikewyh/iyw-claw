import { useEffect } from "react"
import { App as AntApp } from "antd"
import { useLocation } from "react-router-dom"
import { CanvasNodeType, type CanvasNodeData } from "@/types/canvas"
import { useCanvasStore, type CanvasProject } from "@/stores/canvas/use-canvas-store"
import { setImageBlob } from "@/services/image-storage"
import { setMediaBlob } from "@/services/file-storage"
import { getStoredAssetReferences } from "../../../../../widget/src/upstream/plugin-storage"
import type { AssetClient, AssetReference } from "../../../../../widget/src/asset-client"
import type { App } from "@modelcontextprotocol/ext-apps"

type BridgeHost = Pick<App, "callServerTool">
type CanvasScene = {
  canvasId: string
  revision: number
  nodes: Array<Record<string, unknown>>
  connections: CanvasProject["connections"]
  backgroundMode: CanvasProject["backgroundMode"]
  showImageInfo: boolean
  viewport: CanvasProject["viewport"]
  updatedAt: string
}
type BridgeServices = { app: BridgeHost; assets: AssetClient }
declare global { interface Window { __IYW_CANVAS_BRIDGE__?: BridgeServices } }

const SYNC_CANVAS_ID = "main"
const SYNC_INTERVAL_MS = 1200
const SYNC_NAMESPACE = "canvas-agent-sync"
type SyncReceipt = { canvasId: string; revision: number; projectSignature: string }
let legacyHydration: Promise<void> | undefined

export function PluginCanvasBridge() {
  const location = useLocation()
  const { message } = AntApp.useApp()
  const activeProjectId = projectId(location.pathname)
  useEffect(() => {
    const services = window.__IYW_CANVAS_BRIDGE__
    if (!services) return
    const controller = new CanvasSyncController(services, activeProjectId, (text) => message.error(text))
    controller.start()
    return () => controller.stop()
  }, [activeProjectId, message])
  return null
}

class CanvasSyncController {
  private stopped = false
  private timer: ReturnType<typeof setTimeout> | undefined
  private receipt: SyncReceipt | undefined
  private errorMessage = ""
  private readonly app: BridgeHost
  private readonly assets: AssetClient
  constructor(
    services: BridgeServices,
    private readonly projectId: string,
    private readonly notifyError: (text: string) => void,
  ) {
    this.app = services.app
    this.assets = services.assets
  }

  start(): void { void this.pump() }
  stop(): void { this.stopped = true; if (this.timer) clearTimeout(this.timer) }

  private async pump(): Promise<void> {
    try { await this.sync(); this.errorMessage = "" }
    catch (error) { this.report(error) }
    finally { if (!this.stopped) this.timer = setTimeout(() => void this.pump(), SYNC_INTERVAL_MS) }
  }

  private async sync(): Promise<void> {
    if (!useCanvasStore.getState().hydrated) return
    legacyHydration ??= hydrateLegacyMainScene({ app: this.app, assets: this.assets }).catch((error) => { legacyHydration = undefined; throw error })
    await legacyHydration
    if (this.stopped || !this.projectId) return
    const project = useCanvasStore.getState().projects.find((item) => item.id === this.projectId)
    if (!project) return
    const signature = projectSignature(project)
    if (!this.receipt) this.receipt = await readReceipt(this.app, project.id)
    const scene = await readScene(this.app, this.receipt?.canvasId || project.id)
    if (this.stopped) return
    this.receipt = await synchronizeProject({ app: this.app, assets: this.assets, project, scene, signature, receipt: this.receipt })
  }

  private report(error: unknown): void {
    const text = error instanceof Error ? error.message : "Workspace canvas sync failed"
    if (this.errorMessage !== text) this.notifyError(text)
    this.errorMessage = text
  }
}
function projectId(pathname: string): string {
  const match = pathname.match(/^\/canvas\/([^/]+)/)
  return match ? decodeURIComponent(match[1] || "") : ""
}

async function hydrateLegacyMainScene(services: BridgeServices): Promise<void> {
  const store = useCanvasStore.getState()
  if (store.projects.length || store.deletedProjects.length) return
  const scene = await readScene(services.app, SYNC_CANVAS_ID)
  if (!scene.nodes.length) return
  const project = await sceneToProject(scene, services.assets, "main", "Infinite Canvas")
  if (useCanvasStore.getState().projects.length || useCanvasStore.getState().deletedProjects.length) return
  const id = useCanvasStore.getState().importProject(project)
  await writeReceipt(services.app, id, { canvasId: scene.canvasId, revision: scene.revision, projectSignature: projectSignature(project) })
}
async function readReceipt(app: BridgeHost, projectId: string): Promise<SyncReceipt | undefined> {
  const result = await callTool(app, "infinite_canvas_store_data", { operation: "get", namespace: SYNC_NAMESPACE, key: projectId })
  const receipt = result.value as SyncReceipt | undefined
  return receipt && typeof receipt.canvasId === "string" ? receipt : undefined
}
async function writeReceipt(app: BridgeHost, projectId: string, receipt: SyncReceipt): Promise<void> {
  await callTool(app, "infinite_canvas_store_data", { operation: "set", namespace: SYNC_NAMESPACE, key: projectId, value: receipt })
}
function sameSceneContent(left: CanvasScene, right: CanvasScene): boolean {
  return JSON.stringify([left.nodes, left.connections, left.backgroundMode, left.showImageInfo, left.viewport]) ===
    JSON.stringify([right.nodes, right.connections, right.backgroundMode, right.showImageInfo, right.viewport])
}
async function readScene(app: BridgeHost, canvasId: string): Promise<CanvasScene> {
  return callTool(app, "get_infinite_canvas_state", { canvasId }) as Promise<CanvasScene>
}
async function saveScene(app: BridgeHost, scene: CanvasScene): Promise<CanvasScene> {
  return callTool(app, "save_infinite_canvas_snapshot", { canvasId: scene.canvasId, baseRevision: scene.revision, scene }) as Promise<CanvasScene>
}
async function callTool(app: BridgeHost, name: string, arguments_: Record<string, unknown>): Promise<Record<string, unknown>> {
  const result = await app.callServerTool({ name, arguments: arguments_ })
  const text = result.content.find((item) => item.type === "text")?.text
  if (result.isError || !text) throw new Error(text || "Workspace canvas is unavailable")
  return JSON.parse(text) as Record<string, unknown>
}

async function synchronizeProject(input: {
  app: BridgeHost
  assets: AssetClient
  project: CanvasProject
  scene: CanvasScene
  signature: string
  receipt?: SyncReceipt
}): Promise<SyncReceipt> {
  const { app, assets, project, scene, signature, receipt } = input
  if (!receipt) {
    const candidate = await projectToScene(project, scene, app)
    if (!scene.nodes.length) return storeReceipt(app, project.id, signature, await saveScene(app, candidate))
    if (sameSceneContent(candidate, scene)) return storeReceipt(app, project.id, signature, scene)
    throw syncConflict()
  }
  if (scene.revision !== receipt.revision && signature === receipt.projectSignature) {
    await applySceneToProject(scene, project, assets)
    const updated = useCanvasStore.getState().projects.find((item) => item.id === project.id)
    return storeReceipt(app, project.id, projectSignature(updated!), scene)
  }
  if (signature !== receipt.projectSignature && scene.revision === receipt.revision) {
    const candidate = await projectToScene(project, scene, app)
    return storeReceipt(app, project.id, signature, await saveScene(app, candidate))
  }
  if (scene.revision !== receipt.revision && signature !== receipt.projectSignature) throw syncConflict()
  return receipt
}

async function storeReceipt(app: BridgeHost, projectId: string, projectSignature: string, scene: CanvasScene): Promise<SyncReceipt> {
  const receipt = { canvasId: scene.canvasId, revision: scene.revision, projectSignature }
  await writeReceipt(app, projectId, receipt)
  return receipt
}

function syncConflict(): Error {
  return new Error("Canvas changed in another Agent. Reopen the canvas before continuing.")
}

async function projectToScene(project: CanvasProject, scene: CanvasScene, app: BridgeHost): Promise<CanvasScene> {
  const ids = new Map(project.nodes.map((node, index) => [node.id, sceneNodeId(node.id, index)]))
  const [images, media] = await Promise.all([
    getStoredAssetReferences(app, "image_files"),
    getStoredAssetReferences(app, "media_files"),
  ])
  const nodes = await Promise.all(project.nodes.map(async (node) => {
    const metadata = sanitizeMetadata({ ...(node.metadata || {}) })
    const storageKey = typeof metadata.storageKey === "string" ? metadata.storageKey : ""
    const mediaType = node.type === CanvasNodeType.Image ? "image" : node.type === CanvasNodeType.Video ? "video" : node.type === CanvasNodeType.Audio ? "audio" : undefined
    const asset = storageKey ? (mediaType === "image" ? images : media).get(storageKey) : undefined
    if (typeof metadata.content === "string" && metadata.content.startsWith("blob:")) delete metadata.content
    if (asset) metadata.asset = asset
    if (mediaType) metadata.mediaType = mediaType
    if (ids.get(node.id) !== node.id) metadata.originalNodeId = node.id
    const position = node.position || { x: 0, y: 0 }
    return {
      id: ids.get(node.id),
      type: mediaType ? "media" : node.type,
      x: position.x,
      y: position.y,
      width: node.width,
      height: node.height,
      ...(metadata.fontSize ? { fontSize: metadata.fontSize } : {}),
      metadata,
    }
  }))
  return {
    schemaVersion: 1,
    canvasId: scene.canvasId,
    revision: scene.revision,
    nodes,
    connections: project.connections.map((connection) => ({
      ...connection,
      id: sceneNodeId(connection.id, project.connections.indexOf(connection)),
      fromNodeId: ids.get(connection.fromNodeId) || sceneNodeId(connection.fromNodeId, 0),
      toNodeId: ids.get(connection.toNodeId) || sceneNodeId(connection.toNodeId, 0),
    })),
    backgroundMode: project.backgroundMode,
    showImageInfo: project.showImageInfo,
    viewport: project.viewport,
    updatedAt: project.updatedAt,
  } as CanvasScene
}

async function sceneToProject(scene: CanvasScene, assets: AssetClient, id: string, title: string): Promise<CanvasProject> {
  const nodes = await Promise.all(scene.nodes.map((node) => sceneNodeToCanvasNode(node, assets)))
  const now = new Date().toISOString()
  return {
    id,
    title,
    createdAt: now,
    updatedAt: scene.updatedAt,
    nodes,
    connections: scene.connections,
    chatSessions: [],
    activeChatId: null,
    backgroundMode: scene.backgroundMode,
    showImageInfo: scene.showImageInfo,
    viewport: scene.viewport,
  }
}

async function sceneNodeToCanvasNode(node: Record<string, unknown>, assets: AssetClient): Promise<CanvasNodeData> {
  const metadata = { ...((node.metadata as Record<string, unknown> | undefined) || {}) }
  const asset = metadata.asset as AssetReference | undefined
  let type = String(node.type || "text")
  if (asset) {
    const url = await assets.getUrl(asset)
    const blob = await fetch(url).then((response) => response.blob())
    const media = asset.mimeType.startsWith("image/")
      ? { type: CanvasNodeType.Image, storageKey: `image:${asset.sha256}`, url: await setImageBlob(`image:${asset.sha256}`, blob) }
      : asset.mimeType.startsWith("video/")
        ? { type: CanvasNodeType.Video, storageKey: `video:${asset.sha256}`, url: await setMediaBlob(`video:${asset.sha256}`, blob) }
        : { type: CanvasNodeType.Audio, storageKey: `audio:${asset.sha256}`, url: await setMediaBlob(`audio:${asset.sha256}`, blob) }
    type = media.type
    metadata.storageKey = media.storageKey
    metadata.content = media.url
  } else if (type === "media") {
    type = metadata.mediaType === "video" ? CanvasNodeType.Video : CanvasNodeType.Image
  }
  if (typeof metadata.text === "string" && typeof metadata.content !== "string") metadata.content = metadata.text
  return {
    id: String(metadata.originalNodeId || node.id),
    type,
    title: String(metadata.title || type),
    position: { x: Number(node.x) || 0, y: Number(node.y) || 0 },
    width: Number(node.width) || 240,
    height: Number(node.height) || 120,
    metadata: metadata as CanvasNodeData["metadata"],
  }
}

function sceneNodeId(id: string, index: number): string {
  return /^[A-Za-z0-9_-]{1,64}$/.test(id) ? id : `legacy-${index}`
}

function sanitizeMetadata(value: Record<string, unknown>): Record<string, unknown> {
  const metadata = structuredClone(value)
  for (const key of ["content", "dataUrl", "url"]) {
    if (typeof metadata[key] === "string" && /^blob:|^data:/i.test(metadata[key] as string)) delete metadata[key]
  }
  if (Array.isArray(metadata.images)) {
    metadata.images = metadata.images.map((item) => {
      if (!item || typeof item !== "object") return item
      const image = { ...item as Record<string, unknown> }
      for (const key of ["content", "dataUrl", "url"]) {
        if (typeof image[key] === "string" && /^blob:|^data:/i.test(image[key] as string)) delete image[key]
      }
      return image
    })
  }
  return metadata
}

async function applySceneToProject(scene: CanvasScene, project: CanvasProject, assets: AssetClient): Promise<void> {
  const next = await sceneToProject(scene, assets, project.id, project.title)
  const current = useCanvasStore.getState().projects.find((item) => item.id === project.id)
  if (!current || projectSignature(current) !== projectSignature(project)) throw syncConflict()
  useCanvasStore.getState().updateProject(project.id, {
    nodes: next.nodes,
    connections: next.connections,
    backgroundMode: next.backgroundMode,
    showImageInfo: next.showImageInfo,
    viewport: next.viewport,
  })
}

function projectSignature(project: CanvasProject): string {
  const nodes = project.nodes.map((node) => ({ ...node, metadata: sanitizeMetadata(node.metadata || {}) }))
  return JSON.stringify([nodes, project.connections, project.backgroundMode, project.showImageInfo, project.viewport])
}
