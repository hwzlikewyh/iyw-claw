import { mkdir, readFile, rename, unlink, open } from "node:fs/promises"
import { join } from "node:path"
import { CanvasRuntimeError } from "./errors.js"
import { canvasRoot, rejectSymlinkPath, storageRoot } from "./paths.js"
import { withFileLock } from "./lock.js"
import { applyOperations, validateScene } from "./operations.js"
import { defaultScene, type CanvasOperation, type CanvasScene, type CanvasSelection } from "./types.js"

export class SceneStore {
  async exists(canvasId: string): Promise<boolean> {
    try { await readFile(join(canvasRoot(canvasId), "scene.json")); return true } catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return false; throw error }
  }

  async read(canvasId = "main"): Promise<CanvasScene> {
    const root = canvasRoot(canvasId)
    await rejectSymlinkPath(root)
    await mkdir(root, { recursive: true })
    try {
      const scene = JSON.parse(await readFile(join(root, "scene.json"), "utf8")) as CanvasScene
      validateScene(scene)
      return scene
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return defaultScene(canvasId)
      if (error instanceof CanvasRuntimeError) throw error
      throw new CanvasRuntimeError("scene_invalid", "scene JSON is invalid")
    }
  }

  async save(scene: CanvasScene, baseRevision?: number): Promise<CanvasScene> {
    const root = canvasRoot(scene.canvasId)
    await rejectSymlinkPath(root)
    return withFileLock(join(root, ".scene.lock"), async () => {
      const current = await this.read(scene.canvasId)
      if (baseRevision !== undefined && current.revision !== baseRevision) throw new CanvasRuntimeError("revision_conflict", "scene revision changed", { latestRevision: current.revision })
      const next = structuredClone(scene)
      next.revision = current.revision + 1
      next.updatedAt = new Date().toISOString()
      validateScene(next)
      await mkdir(root, { recursive: true })
      await atomicJson(join(root, "scene.json"), next)
      await this.updateIndex(next)
      return next
    })
  }

  async apply(canvasId: string, baseRevision: number, operations: CanvasOperation[]): Promise<CanvasScene> {
    const current = await this.read(canvasId)
    if (current.revision !== baseRevision) throw new CanvasRuntimeError("revision_conflict", "scene revision changed", { latestRevision: current.revision })
    const next = applyOperations(current, operations)
    return this.save(next, baseRevision)
  }

  async readSelection(canvasId = "main"): Promise<CanvasSelection> {
    await rejectSymlinkPath(canvasRoot(canvasId))
    try {
      const value = JSON.parse(await readFile(join(canvasRoot(canvasId), "selection.json"), "utf8")) as CanvasSelection
      if (!Number.isSafeInteger(value.revision) || value.revision < 0 || !Array.isArray(value.selectedNodeIds) || !value.selectedNodeIds.every((id) => typeof id === "string")) throw new Error("selection invalid")
      return value
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw new CanvasRuntimeError("scene_invalid", "selection JSON is invalid")
      return { revision: 0, selectedNodeIds: [], updatedAt: new Date(0).toISOString() }
    }
  }

  async saveSelection(canvasId: string, selection: CanvasSelection): Promise<CanvasSelection> {
    const root = canvasRoot(canvasId)
    await rejectSymlinkPath(root)
    return withFileLock(join(root, ".scene.lock"), async () => {
      const current = await this.read(canvasId)
      if (current.revision !== selection.revision) throw new CanvasRuntimeError("revision_conflict", "scene revision changed", { latestRevision: current.revision })
      const selectedNodeIds = selection.selectedNodeIds.filter((id) => current.nodes.some((node) => node.id === id))
      if (selectedNodeIds.length !== selection.selectedNodeIds.length) throw new CanvasRuntimeError("invalid_input", "selection references a missing node")
      const next = { ...selection, updatedAt: new Date().toISOString() }
      await mkdir(root, { recursive: true })
      await atomicJson(join(root, "selection.json"), next)
      return next
    })
  }

  private async updateIndex(scene: CanvasScene): Promise<void> {
    await rejectSymlinkPath(storageRoot())
    await withFileLock(join(storageRoot(), ".index.lock"), async () => {
      const path = join(storageRoot(), "index.json")
      let index: Record<string, unknown> = {}
      try { index = JSON.parse(await readFile(path, "utf8")) as Record<string, unknown> } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error }
      const canvases = typeof index.canvases === "object" && index.canvases ? index.canvases as Record<string, unknown> : {}
      canvases[scene.canvasId] = { canvasId: scene.canvasId, title: scene.canvasId, updatedAt: scene.updatedAt, revision: scene.revision }
      await mkdir(storageRoot(), { recursive: true })
      await atomicJson(path, { canvases })
    })
  }
}

async function atomicJson(path: string, value: unknown): Promise<void> {
  const temp = `${path}.tmp-${process.pid}-${Math.random().toString(36).slice(2)}`
  try {
    const handle = await open(temp, "w")
    try { await handle.writeFile(`${JSON.stringify(value, null, 2)}\n`, "utf8"); await handle.sync() } finally { await handle.close() }
    await rename(temp, path)
  } finally { await unlink(temp).catch(() => undefined) }
}
