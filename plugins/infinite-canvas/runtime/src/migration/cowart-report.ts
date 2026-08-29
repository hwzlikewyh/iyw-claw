import { mkdir, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { storageRoot } from "../paths.js"
import type { MigrationReport } from "./cowart-types.js"

export async function writeMigrationReport(report: MigrationReport): Promise<string> {
  const directory = join(storageRoot(), "migrations")
  await mkdir(directory, { recursive: true })
  const path = join(directory, `${report.pageId}-${Date.now()}.json`)
  await writeFile(path, `${JSON.stringify(report, null, 2)}\n`, "utf8")
  return path.slice(storageRoot().length + 1).replaceAll("\\", "/")
}
