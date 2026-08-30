import type { CanvasClient, CanvasOperation, CanvasState } from "./canvas-client.js"

export function createSceneClient(client: CanvasClient) {
  return {
    readScene: (sinceRevision?: number) => client.getState(sinceRevision),
    applyOps: (operations: CanvasOperation[], baseRevision: number) => client.apply(operations, baseRevision),
    readSelection: () => client.readSelection(),
    saveSelection: (revision: number, selectedNodeIds: string[]) => client.saveSelection(revision, selectedNodeIds),
    saveSnapshot: (scene: CanvasState, baseRevision: number) => client.saveSnapshot(scene, baseRevision),
  }
}

export type SceneClient = ReturnType<typeof createSceneClient>
export type { CanvasState }
