import { mkdir, rm } from "node:fs/promises"
import { dirname } from "node:path"
import { CanvasRuntimeError } from "./errors.js"

const LOCK_TIMEOUT_MS = 5000
const RETRY_MS = 25

export async function withFileLock<T>(lockPath: string, action: () => Promise<T>): Promise<T> {
  await mkdir(dirname(lockPath), { recursive: true })
  const deadline = Date.now() + LOCK_TIMEOUT_MS
  while (true) {
    try {
      await mkdir(lockPath)
      break
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "EEXIST" || Date.now() >= deadline) {
        throw new CanvasRuntimeError("runtime_unavailable", "canvas lock is unavailable")
      }
      await new Promise((resolvePromise) => setTimeout(resolvePromise, RETRY_MS))
    }
  }
  try {
    return await action()
  } finally {
    await rm(lockPath, { recursive: true, force: true }).catch(() => undefined)
  }
}
