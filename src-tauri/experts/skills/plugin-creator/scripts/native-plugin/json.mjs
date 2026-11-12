import { mkdir, writeFile } from "node:fs/promises"
import path from "node:path"
export async function writeJson(file, value) {
  await mkdir(path.dirname(file), { recursive: true })
  await writeFile(file, `${JSON.stringify(value, null, 2)}\n`, { flag: "wx" })
}
