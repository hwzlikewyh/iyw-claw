import { build, transform } from "esbuild"
import { createHash } from "node:crypto"
import { readFile, writeFile, mkdir, mkdtemp, rm, readdir } from "node:fs/promises"
import { dirname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { tmpdir } from "node:os"

const root = dirname(fileURLToPath(import.meta.url))
const commit = "1846fb35fcca4b5dcc49525089d1030578c3b8e0"
const upstream = "https://raw.githubusercontent.com/RainLib/Foldly/" + commit
const local = process.argv[2]
const output = resolve(root, "../../src-tauri/resources/office-preview")
const files = ["adapter.ts", "hcd.ts", "hcd-parser.ts", "pivot-projection.ts", "rich-text.ts"]
const hashes = {}
const expected = {
  "examples/hdoc/xlsx-univer-viewer/src/adapter.ts": "eb028556e142f00791f529149837744b6d9b878427d29aea077c8c1e31f99eb8",
  "examples/hdoc/xlsx-univer-viewer/src/hcd.ts": "74b5dc7391ccd72bfc51d6af86408772456429ed1a162c0e19ce7e2112705930",
  "examples/hdoc/xlsx-univer-viewer/src/hcd-parser.ts": "d7e7c83a4c926ff2282ec0af1fe307c02b5867d89f3cff801727ba8ba2f398af",
  "examples/hdoc/xlsx-univer-viewer/src/pivot-projection.ts": "d0255448af0bbe867d96d8f969f295df2a355dc8bd41e372025cb2cd20a1d905",
  "examples/hdoc/xlsx-univer-viewer/src/rich-text.ts": "0b5fc1499600a16d09ba431ab313401d6986baf6f83a7a0fa82ffaccd38bc51f",
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

function replaceOnce(text, before, after) {
  if (text.split(before).length !== 2) throw new Error("Upstream preview integration point changed")
  return text.replace(before, after)
}

function adaptDocumentViewer(text) {
  const end = text.indexOf("let activeViewer;")
  if (end < 0) throw new Error("Upstream document viewer entry is missing")
  let result = text.slice(0, end)
  const replacements = [
    ["return new URL(href, this.base);", "return window.officeUrl(new URL(href, this.base));"],
    ["async function fetchChecked(url, kind)", "async function fetchChecked(url, kind, signal)"],
    ["const response = await fetch(url);", "const response = await fetch(url, { signal });"],
    ["async function fetchDecoded(url, limit, kind, expectedHash, expectedLength)", "async function fetchDecoded(url, limit, kind, expectedHash, expectedLength, signal)"],
    ["fetchChecked(url, kind)", "fetchChecked(url, kind, signal)"],
    ["this.resident = 0;", "this.resident = 0; this.abort = new AbortController();"],
    ["fetchDecoded(this.resolve(href), limit, kind, hash, length)", "fetchDecoded(this.resolve(href), limit, kind, hash, length, this.abort.signal)"],
    ["const page = await this.readJson(href, `index page ${pageNumber}`);", "const page = await this.readJson(href, `index page ${pageNumber}`); if (this.abort.signal.aborted) return;"],
    ["const roots = [...template.content.children];", "if (generation !== this.generation) return; const roots = [...template.content.children];"],
    ["close() {", "close() { this.abort.abort();"],
    ["this.tailObserver = new frame.contentWindow.IntersectionObserver", "this.tailObserver = new IntersectionObserver"],
    ["}, { rootMargin: '200% 0px' });", "}, { root: frame, rootMargin: '200% 0px' });"],
  ]
  for (const [before, after] of replacements) result = replaceOnce(result, before, after)
  return result
}

function adaptGridAdapter(text) {
  // 后台工作表的延迟滚动事件不能清空当前工作表的窗口数据。
  const replacements = [
    ["private async ensureVisible(sheet: FWorksheet): Promise<void> {",
      "private async ensureVisible(sheet: FWorksheet): Promise<void> {\n    if (this.disposed || this.workbook.getActiveSheet().getSheetId() !== sheet.getSheetId()) return;"],
    ["private evictOutsideWindow(activeSheetId: string, keep: Set<string>): void {",
      "private evictOutsideWindow(activeSheetId: string, keep: Set<string>): void {\n    if (this.disposed || this.workbook.getActiveSheet().getSheetId() !== activeSheetId) return;"],
    ["window.requestAnimationFrame(() => void this.ensureVisible(sheet));",
      "window.requestAnimationFrame(() => void this.ensureVisible(sheet).catch(error => { if (!this.disposed) this.onError(String(error)); }));"],
    ["const previous = this.runtimes.get(descriptor.chunkId);",
      "if (this.disposed) return;\n      const previous = this.runtimes.get(descriptor.chunkId);"],
    ["await this.applyParsedChunk(sheet, descriptor, parsed);",
      "await this.applyParsedChunk(sheet, descriptor, parsed);\n      if (this.disposed) return;"],
  ]
  for (const [before, after] of replacements) text = replaceOnce(text, before, after)
  return text
}

const cache = await mkdtemp(join(tmpdir(), "iyw-office-resources-"))
try {
  await mkdir(output, { recursive: true })
  for (const file of files) {
    let text = await source(`examples/hdoc/xlsx-univer-viewer/src/${file}`)
    if (file === "hcd.ts") {
      text = replaceOnce(text, "return new URL(href, this.baseUrl);",
        "return window.officeUrl(new URL(href, this.baseUrl));")
    }
    if (file === "adapter.ts") text = adaptGridAdapter(text)
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
  let lazy = adaptDocumentViewer(await source("examples/hdoc/lazy-viewer/viewer.js"))
  lazy += "\n" + await readFile(join(root, "document-viewer.js"), "utf8")
  const compiled = await transform(lazy, { minify: true, target: "chrome109", charset: "utf8" })
  await writeFile(join(output, "viewer.js"), compiled.code)
  await writeFile(join(output, "LICENSE-OfficeCLI.txt"), await source("LICENSE"))
  await writeFile(join(output, "upstream.json"), JSON.stringify({
    repository: "RainLib/Foldly", version: "0.1.32", commit,
    changes: ["Capability-aware asset URLs", "Sandbox-compatible shadow document viewport", "Cancelled stale document requests", "Ignore inactive worksheet eviction and disposed grid callbacks", "Host-driven reload and viewport restoration", "Read-only embedded grid entry"],
    sources: hashes,
  }, null, 2) + "\n")
} finally {
  if (!resolve(cache).startsWith(resolve(tmpdir()) + sep)) throw new Error("Invalid temporary root")
  await rm(cache, { recursive: true, force: true })
}
