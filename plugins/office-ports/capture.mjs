import { mkdir, realpath, writeFile } from "node:fs/promises"
import path from "node:path"
import {
  readNativeSource,
  copyNativeSource,
} from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/source.mjs"
import { buildManifest } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/manifest.mjs"
import { packageStats } from "../../src-tauri/experts/skills/plugin-creator/scripts/native-plugin/package.mjs"
import { inspectPackage } from "./inspect.mjs"

const stage = await realpath(process.argv[2])
const source = await readNativeSource(path.join(stage, "author"))
const output = path.join(stage, "package")
await mkdir(output)
await copyNativeSource(source.root, path.join(output, "source"), [
  source.configPath,
])
const result = await buildManifest(
  { ...source, root: await realpath(path.join(output, "source")) },
  output,
  { minClientVersion: process.argv[3] }
)
const manifestPath = path.join(output, ".iyw-plugin.json")
const { readFile } = await import("node:fs/promises")
const manifest = JSON.parse(await readFile(manifestPath, "utf8"))
// 工具读取用户明确选取的当前工作区文档；由原授权流程确认该范围。
manifest.permissions.workspace.read = ["**"]
manifest.permissions.workspace.write = [".iyw/plugin-imports/**"]
manifest.platforms = [
  "windows-x86_64",
  "windows-aarch64",
  "macos-x86_64",
  "macos-aarch64",
  "linux-x86_64",
  "linux-aarch64",
]
await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`)
const inspection = await inspectPackage(output)
console.log(
  JSON.stringify({ ...result, ...(await packageStats(output)), inspection })
)
