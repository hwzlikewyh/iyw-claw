import { lstat } from "node:fs/promises"
import { dirname, isAbsolute, relative, resolve, sep } from "node:path"
import { CanvasRuntimeError, invalid, type RuntimeErrorCode } from "./errors.js"

const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/

export function requiredEnv(name: string): string {
  const value = process.env[name]?.trim()
  if (!value) throw new CanvasRuntimeError("runtime_unavailable", `${name} is not configured`)
  return value
}

export function storageRoot(): string {
  return resolve(requiredEnv("IYW_WORKSPACE_DIR"), "canvas", "infinite-canvas")
}

export function canvasRoot(canvasId = "main"): string {
  assertId(canvasId)
  return assertWithin(storageRoot(), resolve(storageRoot(), "canvases", canvasId))
}

export function pluginDataRoot(): string {
  return resolve(requiredEnv("IYW_PLUGIN_DATA_DIR"))
}

export function pluginRoot(): string {
  return resolve(requiredEnv("IYW_PLUGIN_ROOT"))
}

export function workspacePath(pathValue: string): string {
  if (!pathValue || isAbsolute(pathValue)) throw invalid("workspace_path_invalid")
  return assertWithin(resolve(requiredEnv("IYW_WORKSPACE_DIR")), resolve(requiredEnv("IYW_WORKSPACE_DIR"), pathValue))
}

export function assertWithin(root: string, candidate: string): string {
  const base = resolve(root)
  const target = resolve(candidate)
  const rel = relative(base, target)
  if (rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel)) throw new CanvasRuntimeError("path_not_allowed", "path escapes its allowed root")
  return target
}

export function assertId(value: string, code: RuntimeErrorCode = "invalid_input"): void {
  if (!ID_PATTERN.test(value)) throw new CanvasRuntimeError(code, "identifier is invalid")
}

export async function rejectSymlink(pathValue: string): Promise<void> {
  let current = resolve(pathValue)
  while (true) {
    const info = await lstat(current)
    if (info.isSymbolicLink()) throw new CanvasRuntimeError("path_not_allowed", "symbolic links are not allowed")
    const parent = dirname(current)
    if (parent === current) return
    current = parent
  }
}

export async function rejectSymlinkPath(pathValue: string): Promise<void> {
  let current = resolve(pathValue)
  while (true) {
    try {
      const info = await lstat(current)
      if (info.isSymbolicLink()) throw new CanvasRuntimeError("path_not_allowed", "symbolic links are not allowed")
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error
    }
    const parent = dirname(current)
    if (parent === current) return
    current = parent
  }
}
