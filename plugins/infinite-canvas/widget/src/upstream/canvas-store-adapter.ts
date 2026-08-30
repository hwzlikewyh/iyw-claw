import type { SceneClient } from "../scene-client.js"
import type { Scene } from "../scene-renderer.js"

export type CanvasProject = Scene & { id: string; title: string }

export function createCanvasStoreAdapter(client: SceneClient) {
  return {
    async createProject(canvasId = "main", title = "Infinite Canvas"): Promise<CanvasProject> {
      const scene = await client.readScene()
      if (!scene || scene.canvasId !== canvasId) throw new Error("canvas project is unavailable")
      return project(scene as Scene, title)
    },
    async openProject(canvasId: string): Promise<CanvasProject> {
      const scene = await client.readScene()
      if (!scene || scene.canvasId !== canvasId) throw new Error("canvas project is unavailable")
      return project(scene as Scene, canvasId)
    },
    async updateProject(current: CanvasProject, patch: Partial<Scene>): Promise<CanvasProject> {
      const { id: _id, title: _title, ...scene } = current
      const next = { ...scene, ...patch, schemaVersion: 1, canvasId: current.canvasId, revision: current.revision }
      const saved = await client.saveSnapshot(next as Scene, current.revision)
      return project(saved as Scene, current.title)
    },
    deleteProjects: () => { throw new Error("canvas project deletion is host-managed") },
  }
}

function project(scene: Scene, title: string): CanvasProject {
  return { ...scene, id: scene.canvasId, title }
}
