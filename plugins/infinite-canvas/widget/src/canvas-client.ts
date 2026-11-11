import { AssetClient } from "./asset-client.js"

export type CanvasOperation = Record<string, unknown>
export type CanvasState = { canvasId: string; revision: number; nodes: Array<Record<string, unknown>>; [key: string]: unknown }
export type ToolCaller = (name: string, args: Record<string, unknown>) => Promise<unknown>

export class CanvasClient {
  private writeQueue: Promise<unknown> = Promise.resolve()
  private pendingWriteCount = 0
  private readonly assets: AssetClient

  constructor(private readonly call: ToolCaller, private readonly getCanvasId: () => string) { this.assets = new AssetClient(call) }

  assetClient(): AssetClient { return this.assets }

  callTool(name: string, args: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.call(name, args) as Promise<Record<string, unknown>>
  }

  hasPendingWrites(): boolean { return this.pendingWriteCount > 0 }

  retainAssets(references: Array<{ sha256: string; bytes: number; mimeType: string }>): void { this.assets.retain(references) }

  async getState(sinceRevision?: number): Promise<CanvasState | null> {
    return this.getStateFor(this.getCanvasId(), sinceRevision)
  }

  private async getStateFor(canvasId: string, sinceRevision?: number): Promise<CanvasState | null> {
    const value = await this.call("get_infinite_canvas_state", { canvasId, ...(sinceRevision === undefined ? {} : { sinceRevision }) }) as Record<string, unknown>
    return value.unchanged ? null : value as unknown as CanvasState
  }

  apply(operations: CanvasOperation[], baseRevision: number): Promise<CanvasState> {
    const canvasId = this.getCanvasId()
    return this.enqueue(async () => {
      try { return await this.applyAt(canvasId, baseRevision, operations) } catch (error) {
        const typed = error as { code?: string }
        if (typed.code !== "revision_conflict") throw error
        const latest = await this.getStateFor(canvasId)
        if (!latest) throw error
        if (!canReplayBatch(operations, latest)) throw error
        return this.applyAt(canvasId, latest.revision, operations)
      }
    })
  }

  saveSelection(revision: number, selectedNodeIds: string[]): Promise<void> {
    const canvasId = this.getCanvasId()
    const ids = [...selectedNodeIds]
    return this.enqueue(async () => {
      for (let attempt = 0; attempt < 2; attempt += 1) {
        const latest = await this.getStateFor(canvasId)
        if (!latest || ids.some((id) => !latest.nodes.some((node) => node.id === id))) throw new Error("selection references a missing node")
        revision = latest.revision
        try { await this.call("save_infinite_canvas_selection", { canvasId, revision, selectedNodeIds: ids }); return }
        catch (error) { if ((error as { code?: string }).code !== "revision_conflict" || attempt) throw error }
      }
    })
  }

  private async applyAt(canvasId: string, baseRevision: number, operations: CanvasOperation[]): Promise<CanvasState> {
    const next = await this.call("apply_infinite_canvas_ops", { canvasId, baseRevision, operations }) as CanvasState
    return next
  }

  setViewport(viewport: { x: number; y: number; k: number }): Promise<CanvasState> {
    const canvasId = this.getCanvasId()
    const value = { ...viewport }
    return this.enqueue(async () => {
      for (let attempt = 0; attempt < 2; attempt += 1) {
        const latest = await this.getStateFor(canvasId)
        if (!latest) throw new Error("canvas state is unavailable")
        const saved = latest.viewport as typeof value | undefined
        if (saved && saved.x === value.x && saved.y === value.y && saved.k === value.k) return latest
        try { return await this.applyAt(canvasId, latest.revision, [{ type: "set_viewport", viewport: value }]) }
        catch (error) { if ((error as { code?: string }).code !== "revision_conflict" || attempt) throw error }
      }
      throw new Error("viewport save did not complete")
    })
  }

  async readSelection(): Promise<{ revision: number; selectedNodeIds: string[] }> {
    return this.call("get_infinite_canvas_selection", { canvasId: this.getCanvasId() }) as Promise<{ revision: number; selectedNodeIds: string[] }>
  }

  saveSnapshot(scene: CanvasState, baseRevision: number): Promise<CanvasState> {
    return this.call("save_infinite_canvas_snapshot", { canvasId: scene.canvasId, baseRevision, scene }) as Promise<CanvasState>
  }

  async upload(blob: Blob, name: string, mimeType = blob.type || "application/octet-stream"): Promise<Record<string, unknown>> {
    return this.assets.upload(blob, name, mimeType) as Promise<Record<string, unknown>>
  }

  async objectUrl(sha256: string, bytes: number, mimeType: string): Promise<string> {
    return this.assets.getUrl({ sha256, bytes, mimeType })
  }

  dispose() {
    this.assets.dispose()
  }

  startPolling(onRefresh: () => Promise<void>): () => void {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = async () => {
      if (!stopped && document.visibilityState === "visible" && this.pendingWriteCount === 0) {
        try { await onRefresh() } catch { stopped = true }
      }
      if (!stopped) timer = setTimeout(tick, 1600)
    }
    timer = setTimeout(tick, 1600)
    return () => { stopped = true; if (timer) clearTimeout(timer) }
  }

  private enqueue<T>(action: () => Promise<T>): Promise<T> {
    this.pendingWriteCount += 1
    const next = this.writeQueue.then(action, action).finally(() => { this.pendingWriteCount -= 1 })
    this.writeQueue = next.catch(() => undefined)
    return next
  }
}

function canReplayBatch(operations: CanvasOperation[], scene: CanvasState): boolean {
  const nodes = new Set(scene.nodes.map((node) => node.id))
  const connections = new Set((scene.connections as Array<Record<string, unknown>> | undefined)?.map((connection) => connection.id))
  for (const operation of operations) {
    if (operation.type === "add_node") { const node = operation.node as Record<string, unknown>; if (typeof node?.id !== "string" || nodes.has(node.id)) return false; nodes.add(node.id); continue }
    // 节点覆盖/删除不能仅凭 ID 存在就在新 revision 上重放。
    if (operation.type === "add_connection") { const connection = operation.connection as Record<string, unknown>; if (typeof connection?.id !== "string" || connections.has(connection.id) || !nodes.has(connection.fromNodeId) || !nodes.has(connection.toNodeId)) return false; connections.add(connection.id); continue }
    return false
  }
  return true
}
