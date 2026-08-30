import { mkdir, open, rename, unlink } from "node:fs/promises"
import { randomUUID } from "node:crypto"
import { join } from "node:path"
import { rejectSymlinkPath } from "../paths.js"
import { storageRoot } from "../paths.js"
import type { MigrationReport } from "./cowart-types.js"

export async function writeMigrationReport(report: MigrationReport): Promise<string> {
  const directory = join(storageRoot(), "migrations")
  await rejectSymlinkPath(directory)
  await mkdir(directory, { recursive: true })
  const path = join(directory, `${report.pageId}-${Date.now()}.json`)
  await rejectSymlinkPath(path)
  const temporary = `${path}.tmp-${process.pid}-${randomUUID()}`
  try {
    const handle = await open(temporary, "wx")
    try { await handle.writeFile(`${JSON.stringify(report, null, 2)}\n`, "utf8"); await handle.sync() } finally { await handle.close() }
    await rename(temporary, path)
  } finally { await unlink(temporary).catch(() => undefined) }
  return path.slice(storageRoot().length + 1).replaceAll("\\", "/")
}
