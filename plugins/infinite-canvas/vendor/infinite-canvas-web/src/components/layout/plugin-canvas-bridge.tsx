import { applySceneToProject, projectSignature, projectToScene, sceneToProject, syncConflict, type CanvasScene } from "./plugin-canvas-scene"
import { useEffect } from "react"
import { App as AntApp } from "antd"
import { useLocation } from "react-router-dom"
import { useCanvasStore, type CanvasProject } from "@/stores/canvas/use-canvas-store"
import type { AssetClient } from "../../../../../widget/src/asset-client"
import type { App } from "@modelcontextprotocol/ext-apps"

type BridgeHost = Pick<App, "callServerTool">
type BridgeServices = { app: BridgeHost; assets: AssetClient }

const SYNC_CANVAS_ID = "main"
const SYNC_INTERVAL_MS = 1200
const SYNC_NAMESPACE = "canvas-agent-sync"
type SyncReceipt = { canvasId: string; revision: number; projectSignature: string }
let legacyHydration: Promise<void> | undefined
let activeController: CanvasSyncController | undefined

export async function flushHostCanvas(projectId: string): Promise<CanvasScene> {
  const controller = activeController
  if (!controller || controller.projectId !== projectId) throw syncConflict()
  return controller.flush()
}

export function PluginCanvasBridge() {
  const location = useLocation()
  const { message } = AntApp.useApp()
  const activeProjectId = projectId(location.pathname)
  useEffect(() => {
    const services = window.__IYW_CANVAS_BRIDGE__
    if (!services) return
    const controller = new CanvasSyncController(services, activeProjectId, (text) => message.error(text))
    activeController = controller
    controller.start()
    return () => { controller.stop(); if (activeController === controller) activeController = undefined }
  }, [activeProjectId, message])
  return null
}

class CanvasSyncController {
  private stopped = false
  private timer: ReturnType<typeof setTimeout> | undefined
  private receipt: SyncReceipt | undefined
  private errorMessage = ""
  private queued: Promise<void> = Promise.resolve()
  private readonly app: BridgeHost
  private readonly assets: AssetClient
  constructor(
    services: BridgeServices,
    readonly projectId: string,
    private readonly notifyError: (text: string) => void,
  ) {
    this.app = services.app
    this.assets = services.assets
  }

  start(): void { void this.pump() }
  stop(): void { this.stopped = true; if (this.timer) clearTimeout(this.timer) }

  async flush(): Promise<CanvasScene> {
    await this.synchronize()
    const project = useCanvasStore.getState().projects.find((item) => item.id === this.projectId)
    if (this.stopped || !project || !this.receipt || projectSignature(project) !== this.receipt.projectSignature) throw syncConflict()
    return readScene(this.app, this.receipt.canvasId)
  }

  private synchronize(): Promise<void> {
    this.queued = this.queued.catch(() => undefined).then(() => this.sync())
    return this.queued
  }

  private async pump(): Promise<void> {
    try { await this.synchronize(); this.errorMessage = "" }
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
  const project = await sceneToProject(scene, services.assets, { id: "main", title: "Infinite Canvas" })
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
