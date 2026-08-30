import { createHash } from "node:crypto"
import { readdir, readFile, stat } from "node:fs/promises"
import { join } from "node:path"
import { CanvasRuntimeError } from "../errors.js"
import { requiredEnv, assertId, assertWithin, rejectSymlink } from "../paths.js"
import type { CowartPage, CowartRecord } from "./cowart-types.js"

const MAX_SOURCE_BYTES = 50 * 1024 * 1024

export async function readCowartPage(pageId: string): Promise<CowartPage> {
  assertId(pageId)
  const workspace = requiredEnv("IYW_WORKSPACE_DIR")
  const directory = assertWithin(workspace, join(workspace, "canvas", "pages", pageId))
  const sourcePath = join(directory, "cowart-canvas.json")
  await rejectSymlink(sourcePath)
  const info = await stat(sourcePath)
  if (!info.isFile() || info.size > MAX_SOURCE_BYTES) throw new CanvasRuntimeError("invalid_input", "Cowart source file is invalid")
  const bytes = await readFile(sourcePath)
  const sourceSha256 = createHash("sha256").update(bytes).digest("hex")
  let value: Record<string, unknown>
  try {
    const parsed = JSON.parse(bytes.toString("utf8"))
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("root is not an object")
    value = parsed as Record<string, unknown>
  } catch (error) {
    throw new CanvasRuntimeError("invalid_input", "Cowart source JSON is invalid", { reason: error instanceof Error ? error.message : "parse_failed" })
  }
  const rawRecords = Array.isArray(value.records) ? value.records : Array.isArray((value.document as Record<string, unknown> | undefined)?.records) ? ((value.document as Record<string, unknown>).records as unknown[]) : []
  const warnings: string[] = []
  const records = rawRecords.flatMap((raw, index) => {
    const record = parseRecord(raw)
    if (record) return [record]
    warnings.push(`record_${index}_invalid`)
    return []
  })
  return { pageId, sourcePath: `canvas/pages/${pageId}/cowart-canvas.json`, sourceDirectory: `canvas/pages/${pageId}`, sourceSha256, records, warnings }
}

export async function listCowartPages(): Promise<Array<{ pageId: string; sourcePath: string; sourceSha256: string; bytes: number; updatedAt: string }>> {
  const pagesRoot = assertWithin(requiredEnv("IYW_WORKSPACE_DIR"), join(requiredEnv("IYW_WORKSPACE_DIR"), "canvas", "pages"))
  const entries = await readdir(pagesRoot, { withFileTypes: true }).catch((error: unknown) => {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return []
    throw error
  })
  const pages: Array<{ pageId: string; sourcePath: string; sourceSha256: string; bytes: number; updatedAt: string }> = []
  for (const entry of entries) {
    if (!entry.isDirectory() || !/^[A-Za-z0-9_-]{1,64}$/.test(entry.name)) continue
    const source = join(pagesRoot, entry.name, "cowart-canvas.json")
    try {
      await rejectSymlink(source)
      const info = await stat(source)
      if (!info.isFile() || info.size > MAX_SOURCE_BYTES) continue
      const bytes = await readFile(source)
      pages.push({ pageId: entry.name, sourcePath: `canvas/pages/${entry.name}/cowart-canvas.json`, sourceSha256: createHash("sha256").update(bytes).digest("hex"), bytes: info.size, updatedAt: info.mtime.toISOString() })
    } catch { continue }
  }
  return pages.sort((left, right) => left.pageId.localeCompare(right.pageId))
}

function parseRecord(value: unknown): CowartRecord | null {
  if (!value || typeof value !== "object") return null
  const raw = value as Record<string, unknown>
  if (typeof raw.id !== "string" || !raw.id || raw.id.length > 128) return null
  return { id: raw.id, ...optionalString(raw, "typeName"), ...optionalString(raw, "type"), ...optionalString(raw, "parentId"), ...optionalNumber(raw, "x"), ...optionalNumber(raw, "y"), ...optionalNumber(raw, "rotation"), props: safeObject(raw.props), meta: safeObject(raw.meta) }
}

function optionalString(value: Record<string, unknown>, key: string): Partial<CowartRecord> { return typeof value[key] === "string" ? { [key]: value[key] as string } : {} }
function optionalNumber(value: Record<string, unknown>, key: string): Partial<CowartRecord> { return typeof value[key] === "number" && Number.isFinite(value[key]) ? { [key]: value[key] as number } : {} }
function safeObject(value: unknown): Record<string, unknown> | undefined { if (!value || typeof value !== "object" || Array.isArray(value)) return undefined; return Object.fromEntries(Object.entries(value as Record<string, unknown>).filter(([key]) => !["__proto__", "prototype", "constructor"].includes(key))) }
