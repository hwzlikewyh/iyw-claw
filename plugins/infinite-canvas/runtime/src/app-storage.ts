import { createHash, randomUUID } from "node:crypto"
import { open, readFile, readdir, rename, unlink } from "node:fs/promises"
import { join } from "node:path"
import { CanvasRuntimeError, invalid } from "./errors.js"
import { withFileLock } from "./lock.js"
import { rejectSymlinkPath, storageRoot } from "./paths.js"

type RecordValue = { key: string; value: unknown }

// 保持与宿主 plugin_app_storage 的路径和 JSON 格式一致，升级客户端不迁移数据。
export async function appStorage(args: Record<string, unknown>): Promise<unknown> {
  const operation = args.operation
  if (!["get", "set", "delete", "list"].includes(String(operation))) throw invalid("storage_operation_invalid")
  const namespace = storageKey(args.namespace, 160)
  const directory = join(storageRoot(), "app-data", digest(namespace))
  await rejectSymlinkPath(directory)
  if (operation === "list") return { entries: await listEntries(directory) }
  const key = storageKey(args.key, 240)
  const path = join(directory, `${digest(key)}.json`)
  await rejectSymlinkPath(path)
  if (operation === "get") return { value: (await readEntry(path, key))?.value ?? null }
  if (operation === "set" && args.value === undefined) throw invalid("storage_value_missing")
  await withFileLock(join(directory, ".storage.lock"), async () => {
    await rejectSymlinkPath(path)
    if (operation === "set") await saveEntry(path, { key, value: args.value })
    else await unlink(path).catch((error: NodeJS.ErrnoException) => { if (error.code !== "ENOENT") throw error })
  })
  return operation === "set" ? { key, saved: true } : { key, deleted: true }
}

function storageKey(value: unknown, limit: number): string {
  if (typeof value !== "string" || !value || Buffer.byteLength(value, "utf8") > limit || /\p{Cc}/u.test(value)) throw invalid("storage_key_invalid")
  return value
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex")
}

async function readEntry(path: string, key?: string): Promise<RecordValue | null> {
  await rejectSymlinkPath(path)
  let text: string
  try { text = await readFile(path, "utf8") }
  catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return null; throw error }
  try {
    const entry = JSON.parse(text) as RecordValue
    if (!entry || typeof entry.key !== "string" || !("value" in entry) || (key !== undefined && entry.key !== key)) throw invalid("storage_record_invalid")
    return entry
  } catch {
    throw new CanvasRuntimeError("runtime_unavailable", "画布存储文件损坏，已保留原文件，请从备份恢复。")
  }
}

async function listEntries(directory: string): Promise<RecordValue[]> {
  let names: string[]
  try { names = await readdir(directory) }
  catch (error) { if ((error as NodeJS.ErrnoException).code === "ENOENT") return []; throw error }
  const entries: RecordValue[] = []
  for (const name of names.sort()) {
    if (!/^[a-f0-9]{64}\.json$/.test(name)) continue
    const entry = await readEntry(join(directory, name))
    if (entry) entries.push(entry)
  }
  return entries
}

async function saveEntry(path: string, value: RecordValue): Promise<void> {
  const temporary = `${path}.${randomUUID()}.tmp`
  try {
    const file = await open(temporary, "wx")
    try { await file.writeFile(JSON.stringify(value) + "\n", "utf8"); await file.sync() }
    finally { await file.close() }
    await rename(temporary, path)
  } finally { await unlink(temporary).catch(() => undefined) }
}
