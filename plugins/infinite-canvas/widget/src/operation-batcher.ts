import type { CanvasOperation } from "./canvas-client.js"

export function mergeOperations(operations: CanvasOperation[]): CanvasOperation[] {
  const result: CanvasOperation[] = []
  for (const operation of operations) {
    const previous = result.at(-1)
    if (operation.type === "update_node" && previous?.type === "update_node" && operation.nodeId === previous.nodeId) {
      const previousPatch = isRecord(previous.patch) ? previous.patch : {}
      const currentPatch = isRecord(operation.patch) ? operation.patch : {}
      result[result.length - 1] = { type: "update_node", nodeId: operation.nodeId, patch: { ...previousPatch, ...currentPatch } }
      continue
    }
    if (operation.type === "remove_node" && previous?.type === "add_node" && isRecord(previous.node) && operation.nodeId === previous.node.id) {
      result.pop()
      continue
    }
    result.push(operation)
  }
  return result
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value && typeof value === "object" && !Array.isArray(value))
}

export class OperationBatcher {
  private pending: CanvasOperation[] = []
  private running: Promise<unknown> | undefined
  private stopped = false

  add(operation: CanvasOperation): void {
    if (!this.stopped) this.pending = mergeOperations([...this.pending, operation])
  }

  async flush(send: (operations: CanvasOperation[]) => Promise<unknown>): Promise<void> {
    if (this.stopped) return
    if (this.running) { await this.running; return this.flush(send) }
    if (!this.pending.length) return
    const batch = this.pending
    this.pending = []
    this.running = send(batch).catch((error) => { if (!this.stopped) this.pending = mergeOperations([...batch, ...this.pending]); throw error }).finally(() => { this.running = undefined })
    await this.running
  }

  async dispose(): Promise<void> {
    this.stopped = true
    this.pending = []
    await this.running?.catch(() => undefined)
  }
}
