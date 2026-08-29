export type CanvasOperation = Record<string, unknown>
export type CanvasState = { canvasId: string; revision: number; nodes: Array<Record<string, unknown>>; [key: string]: unknown }
export type ToolCaller = (name: string, args: Record<string, unknown>) => Promise<unknown>

const CHUNK_BYTES = 128 * 1024

export class CanvasClient {
  private writeQueue: Promise<unknown> = Promise.resolve()
  private readonly objectUrls = new Map<string, string>()

  constructor(private readonly call: ToolCaller, private readonly getCanvasId: () => string) {}

  async getState(sinceRevision?: number): Promise<CanvasState | null> {
    const value = await this.call("get_infinite_canvas_state", { canvasId: this.getCanvasId(), ...(sinceRevision === undefined ? {} : { sinceRevision }) }) as Record<string, unknown>
    return value.unchanged ? null : value as unknown as CanvasState
  }

  apply(operations: CanvasOperation[], baseRevision: number): Promise<CanvasState> {
    return this.enqueue(async () => {
      try { return await this.call("apply_infinite_canvas_ops", { canvasId: this.getCanvasId(), baseRevision, operations }) as CanvasState } catch (error) {
        const typed = error as { code?: string }
        if (typed.code !== "revision_conflict") throw error
        const latest = await this.getState()
        if (!latest) throw error
        const replayable = operations.filter((operation) => canReplay(operation, latest))
        if (!replayable.length) throw error
        return await this.call("apply_infinite_canvas_ops", { canvasId: this.getCanvasId(), baseRevision: latest.revision, operations: replayable }) as CanvasState
      }
    })
  }

  saveSelection(revision: number, selectedNodeIds: string[]): Promise<void> {
    return this.enqueue(async () => { await this.call("save_infinite_canvas_selection", { canvasId: this.getCanvasId(), revision, selectedNodeIds }) })
  }

  async upload(blob: Blob, name: string, mimeType = blob.type || "application/octet-stream"): Promise<Record<string, unknown>> {
    const bytes = new Uint8Array(await blob.arrayBuffer())
    if (!bytes.byteLength) throw new Error("asset is empty")
    const digest = await crypto.subtle.digest("SHA-256", bytes)
    const expectedSha256 = [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("")
    const started = await this.call("write_infinite_canvas_asset", { name, mimeType, expectedBytes: bytes.byteLength, expectedSha256 }) as { uploadId: string }
    for (let offset = 0, chunkIndex = 0; offset < bytes.length; offset += CHUNK_BYTES, chunkIndex += 1) {
      const dataBase64 = toBase64(bytes.subarray(offset, offset + CHUNK_BYTES))
      await this.call("write_infinite_canvas_asset", { uploadId: started.uploadId, chunkIndex, dataBase64 })
    }
    return this.call("write_infinite_canvas_asset", { uploadId: started.uploadId, finalize: true }) as Promise<Record<string, unknown>>
  }

  async objectUrl(sha256: string, bytes: number, mimeType: string): Promise<string> {
    const cached = this.objectUrls.get(sha256)
    if (cached) return cached
    const parts: Uint8Array[] = []
    for (let offset = 0; offset < bytes; offset += CHUNK_BYTES) {
      const value = await this.call("read_infinite_canvas_asset", { sha256, offset, length: CHUNK_BYTES }) as { dataBase64: string }
      parts.push(fromBase64(value.dataBase64))
    }
    const url = URL.createObjectURL(new Blob(parts, { type: mimeType }))
    this.objectUrls.set(sha256, url)
    return url
  }

  dispose() {
    for (const url of this.objectUrls.values()) URL.revokeObjectURL(url)
    this.objectUrls.clear()
  }

  startPolling(onRefresh: () => Promise<void>): () => void {
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const tick = async () => {
      if (!stopped && document.visibilityState === "visible") await onRefresh().catch(() => undefined)
      if (!stopped) timer = setTimeout(tick, 1600)
    }
    timer = setTimeout(tick, 1600)
    return () => { stopped = true; if (timer) clearTimeout(timer) }
  }

  private enqueue<T>(action: () => Promise<T>): Promise<T> {
    const next = this.writeQueue.then(action, action)
    this.writeQueue = next.catch(() => undefined)
    return next
  }
}

function toBase64(bytes: Uint8Array): string {
  let value = ""
  for (let offset = 0; offset < bytes.length; offset += 0x8000) value += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000))
  return btoa(value)
}

function fromBase64(value: string): Uint8Array {
  const binary = atob(value)
  return Uint8Array.from(binary, (character) => character.charCodeAt(0))
}

function canReplay(operation: CanvasOperation, scene: CanvasState): boolean {
  const nodes = new Set(scene.nodes.map((node) => node.id))
  const connections = new Set((scene.connections as Array<Record<string, unknown>> | undefined)?.map((connection) => connection.id))
  if (operation.type === "add_node") return typeof (operation.node as Record<string, unknown>)?.id === "string" && !nodes.has((operation.node as Record<string, unknown>).id as string)
  if (operation.type === "update_node" || operation.type === "remove_node") return typeof operation.nodeId === "string" && nodes.has(operation.nodeId)
  if (operation.type === "add_connection") { const connection = operation.connection as Record<string, unknown>; return typeof connection?.id === "string" && !connections.has(connection.id) && nodes.has(connection.fromNodeId) && nodes.has(connection.toNodeId) }
  if (operation.type === "remove_connection") return typeof operation.connectionId === "string" && connections.has(operation.connectionId)
  return operation.type === "set_viewport"
}
