import { build, transform } from "esbuild"
import { createHash } from "node:crypto"
import { readFile, writeFile, mkdir, mkdtemp, rm, readdir } from "node:fs/promises"
import { dirname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { tmpdir } from "node:os"

const root = dirname(fileURLToPath(import.meta.url))
const commit = "7916bb354aaa9b25cb7c3d5270a6a2f441d84878"
const upstream = "https://raw.githubusercontent.com/RainLib/OfficeCLI-rust/" + commit
const local = process.argv[2]
const output = resolve(root, "../../src-tauri/resources/office-preview")
const files = ["adapter.ts", "hcd.ts", "hcd-parser.ts"]
const hashes = {}
const expected = {
  "examples/hdoc/xlsx-univer-viewer/src/adapter.ts": "08d1ec562d7c1c50c8a271ec4887426ac89178c2b4b92894937e217985e2833e",
  "examples/hdoc/xlsx-univer-viewer/src/hcd.ts": "50c3c56ba5dbe496368df351f896fed1a85c4a72b54d789c2ad9e95ecd822e2b",
  "examples/hdoc/xlsx-univer-viewer/src/hcd-parser.ts": "8b7bed73901220cc4bbb843e7a95afd7a80c040732a83b96ffa13e37990d718e",
  "examples/hdoc/lazy-viewer/viewer.js": "fe233b61e9497ea8d5f0a0b8467a257945cfef9d19784e9eff9dadb93192909d",
  "LICENSE": "01ac3e9a41fbe85d8311b49ab0b5e124a41cfee921bb5caf98023707a9270ee2",
}

async function source(file) {
  let bytes
  if (local) bytes = await readFile(join(local, file))
  else {
    const response = await fetch(`${upstream}/${file}`, {
      signal: AbortSignal.timeout(30000),
    })
    if (!response.ok) throw new Error(`Upstream ${file}: ${response.status}`)
    bytes = Buffer.from(await response.arrayBuffer())
  }
  bytes = Buffer.from(bytes.toString("utf8").replaceAll("\r\n", "\n"))
  hashes[file] = createHash("sha256").update(bytes).digest("hex")
  if (hashes[file] !== expected[file]) throw new Error(`Pinned source hash mismatch: ${file}`)
  return bytes.toString("utf8")
}

const cache = await mkdtemp(join(tmpdir(), "iyw-office-resources-"))
try {
  await mkdir(output, { recursive: true })
  for (const file of files) {
    let text = await source(`examples/hdoc/xlsx-univer-viewer/src/${file}`)
    if (file === "hcd.ts") {
      text = text.replace("return new URL(href, this.baseUrl);",
        "return window.officeUrl(new URL(href, this.baseUrl));")
    }
    await writeFile(join(cache, file), text)
  }
  await writeFile(join(cache, "grid.ts"), await readFile(join(root, "grid.ts.in")))
  const built = await build({
    entryPoints: [join(cache, "grid.ts")],
    nodePaths: [join(root, "node_modules")],
    outfile: join(output, "grid.js"),
    bundle: true, minify: true, format: "iife", target: "chrome109",
    legalComments: "external", charset: "utf8",
    metafile: true,
    define: { "process.env.NODE_ENV": '"production"' },
  })
  const packages = new Set()
  for (const input of Object.keys(built.metafile.inputs)) {
    const match = input.replaceAll("\\", "/").match(/node_modules\/((?:@[^/]+\/)?[^/]+)/)
    if (match) packages.add(match[1])
  }
  const notices = []
  for (const name of [...packages].sort()) {
    const dir = join(root, "node_modules", name)
    const names = await readdir(dir)
    for (const file of names.filter((name) => /^(?:license|licence|copying|notice)(?:\.|$)/i.test(name))) {
      notices.push({ package: name, file, text: await readFile(join(dir, file), "utf8") })
    }
  }
  await writeFile(join(output, "licenses.json"), JSON.stringify(notices))
  let lazy = await source("examples/hdoc/lazy-viewer/viewer.js")
  lazy = lazy.slice(0, lazy.indexOf("let activeViewer;"))
  lazy = lazy.replace("return new URL(href, this.base);",
    "return window.officeUrl(new URL(href, this.base));")
  lazy += `\nlet activeViewer; window.officeLoad = async function(base) {
    const top = frame.contentWindow?.scrollY || 0;
    activeViewer?.close(); activeViewer = new LazyHcdViewer(new URL(base), undefined, 12);
    await activeViewer.open(); frame.contentWindow?.scrollTo(0, top);
  };`
  const compiled = await transform(lazy, { minify: true, target: "chrome109", charset: "utf8" })
  await writeFile(join(output, "viewer.js"), compiled.code)
  await writeFile(join(output, "LICENSE-OfficeCLI.txt"), await source("LICENSE"))
  await writeFile(join(output, "upstream.json"), JSON.stringify({
    repository: "RainLib/OfficeCLI-rust", version: "0.1.31", commit,
    changes: ["Capability-aware asset URLs", "Host-driven reload and viewport restoration", "Read-only embedded grid entry"],
    sources: hashes,
  }, null, 2) + "\n")
} finally {
  if (!resolve(cache).startsWith(resolve(tmpdir()) + sep)) throw new Error("Invalid temporary root")
  await rm(cache, { recursive: true, force: true })
}
