import { mkdir, writeFile } from "node:fs/promises"
import { contracts } from "../shared/contracts-data.mjs"

const root = new URL("../contracts/", import.meta.url)
await mkdir(root, { recursive: true })
for (const value of Object.values(contracts).sort((left, right) => left.schemaPath.localeCompare(right.schemaPath))) {
  await writeFile(new URL(value.schemaPath.replace(/^contracts\//, ""), root), `${JSON.stringify(value.inputSchema, null, 2)}\n`, "utf8")
}
