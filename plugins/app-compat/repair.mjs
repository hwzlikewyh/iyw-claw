import { cp, lstat, readFile, writeFile } from "node:fs/promises"
import path from "node:path"
import { repairCowart } from "./cowart.mjs"
import {
  repairExcalidraw,
  repairInfiniteCanvas,
  repairOffice,
} from "./patches.mjs"

const repairs = {
  cowart: ["0.1.28", "0.1.29", repairCowart],
  "infinite-canvas": ["0.1.22", "0.1.23", repairInfiniteCanvas],
  "iyw-excalidraw": ["0.3.6", "0.3.7", repairExcalidraw],
  "iyw-documents": ["0.1.0", "0.1.1", repairOffice],
  "iyw-pdf": ["0.1.0", "0.1.1", repairOffice],
  "iyw-presentations": ["0.1.0", "0.1.1", repairOffice],
  "iyw-spreadsheets": ["0.1.0", "0.1.1", repairOffice],
}

const [sourceArgument, outputArgument] = process.argv.slice(2)
if (!sourceArgument || !outputArgument)
  throw Error(
    "Usage: node repair.mjs <published-package> <new-output-directory>"
  )
const source = path.resolve(sourceArgument),
  output = path.resolve(outputArgument)
const relative = path.relative(source, output)
if (!relative || (!relative.startsWith("..") && !path.isAbsolute(relative)))
  throw Error("Output must be outside the published package")
const manifest = JSON.parse(
  await readFile(path.join(source, ".iyw-plugin.json"), "utf8")
)
const repair = repairs[manifest.name]
if (!repair || manifest.version !== repair[0])
  throw Error("Unsupported published version")
await lstat(output).then(
  () => {
    throw Error("Output already exists")
  },
  (error) => {
    if (error.code !== "ENOENT") throw error
  }
)
await cp(source, output, { recursive: true, errorOnExist: true, force: false })
await repair[2](output)
manifest.version = repair[1]
manifest.minClientVersion ||= "0.1.363"
await writeFile(
  path.join(output, ".iyw-plugin.json"),
  JSON.stringify(manifest, null, 2) + "\n"
)
await writeFile(
  path.join(output, "IYW_COMPAT_CHANGES.md"),
  `# ${manifest.version}\n\n修复插件资源访问与关闭兼容，保留既有权限、依赖、许可证和市场身份。\n`
)
console.log(
  JSON.stringify({
    name: manifest.name,
    version: manifest.version,
    directory: output,
  })
)
