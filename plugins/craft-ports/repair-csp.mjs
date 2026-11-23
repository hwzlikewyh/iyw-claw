import {
  cp,
  mkdir,
  readdir,
  readFile,
  realpath,
  writeFile,
} from "node:fs/promises"
import path from "node:path"

const PREVIOUS_VERSION = "0.2.5"
const VERSION = "0.2.6"
const NAMES = new Set([
  "cadcraft",
  "deckcraft",
  "designcraft",
  "gridcraft",
  "soundcraft",
  "vectorcraft",
  "wordcraft",
])
const LEGACY_METADATA =
  'ui: { prefersBorder: false, backgroundRetention: true },\n  "zcode/csp": { wasmUnsafeEval: true }'
const CSP = { wasmUnsafeEval: true, resourceDomains: ["blob:", "data:"] }
const [source, destination] = process.argv.slice(2)
if (!source || !destination || process.argv.length !== 4)
  throw Error("Use repair-csp.mjs <published-package> <new-package-directory>")
await repair(await realpath(source), path.resolve(destination))

async function repair(input, output) {
  const manifest = await readJson(input, ".iyw-plugin.json")
  const native = await readJson(input, "native-plugin.json")
  const name = native.source.name
  if (
    !NAMES.has(name) ||
    manifest.name !== `iyw-zcode-${name}` ||
    manifest.version !== PREVIOUS_VERSION ||
    Object.keys(native.connectors).length !== 1
  )
    throw Error("Expected the published Craft 0.2.5 package")
  const serverFile = "source/dist/server.mjs"
  const server = await readFile(path.join(input, serverFile), "utf8")
  if (server.split(LEGACY_METADATA).length !== 2)
    throw Error("Craft metadata changed; review before repackaging")
  repairConnector(native.connectors[name])
  manifest.version = VERSION
  const relative = path.relative(input, output)
  if (!relative.startsWith("..") && !path.isAbsolute(relative))
    throw Error("Output must stay outside the original package")
  // 新目录专用于本次候选，拒绝覆盖已有包。
  await mkdir(output)
  for (const entry of await readdir(input))
    await cp(path.join(input, entry), path.join(output, entry), {
      recursive: true,
      force: false,
      errorOnExist: true,
    })
  await writeFile(
    path.join(output, serverFile),
    server.replace(
      LEGACY_METADATA,
      `ui: { prefersBorder: false, backgroundRetention: true, csp: ${JSON.stringify(CSP)} }`
    )
  )
  await repairPanel(output)
  await writeJson(output, ".iyw-plugin.json", manifest)
  await writeJson(output, "native-plugin.json", native)
  console.log(JSON.stringify({ name: manifest.name, version: VERSION, output }))
}

async function repairPanel(directory) {
  const file = path.join(directory, "source/dist/panel.html")
  const text = await readFile(file, "utf8")
  await writeFile(file, text.replaceAll("ZCode", "IYW"))
}

function repairConnector(connector) {
  const panels = connector?.resources.filter((resource) =>
    resource.mimeType?.startsWith("text/html")
  )
  if (
    panels?.length !== 1 ||
    panels[0]._meta?.["zcode/csp"]?.wasmUnsafeEval !== true ||
    panels[0]._meta.ui?.csp ||
    connector.wasmUnsafeEval !== false
  )
    throw Error("Unexpected captured Craft resource metadata")
  const { "zcode/csp": legacy, ...metadata } = panels[0]._meta
  if (!legacy.wasmUnsafeEval) throw Error("Missing upstream WASM declaration")
  panels[0]._meta = { ...metadata, ui: { ...metadata.ui, csp: CSP } }
  connector.wasmUnsafeEval = true
}

async function readJson(directory, file) {
  return JSON.parse(await readFile(path.join(directory, file), "utf8"))
}

async function writeJson(directory, file, value) {
  await writeFile(
    path.join(directory, file),
    JSON.stringify(value, null, 2) + "\n"
  )
}
