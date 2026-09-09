import { createHash } from "node:crypto"
import {
  cpSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  writeFileSync,
} from "node:fs"
import { isAbsolute, join, relative, resolve, sep } from "node:path"

const WORKER_ROOT = "src-tauri/resources/xinghe-worker"
const WORKER_FILES = [
  "iyw_xinghe_worker.dll",
  "iyw-xinghe-helper.exe",
  "xinghe-command-runner.exe",
  "xinghe-windows-sandbox-setup.exe",
]

export function windowsLayout(target) {
  if (!["x86_64-pc-windows-msvc", "i686-pc-windows-msvc"].includes(target)) {
    throw new Error(`unsupported Windows staging target: ${target}`)
  }
  const x64 = target === "x86_64-pc-windows-msvc"
  const binary = `src-tauri/target/${target}/release/iyw-claw.exe`
  return {
    arch: x64 ? "x64" : "x86",
    binary,
    directories: [
      "out",
      WORKER_ROOT,
      ...(x64
        ? ["src-tauri/binaries", "src-tauri/resources/runtime-seed"]
        : []),
    ],
    files: [binary, ...(x64 ? ["src-tauri/tauri.runtime-seed.conf.json"] : [])],
    required: [
      binary,
      ...WORKER_FILES.map((name) => `${WORKER_ROOT}/${name}`),
      ...(x64
        ? [
            "src-tauri/tauri.runtime-seed.conf.json",
            `src-tauri/binaries/agent-browser-${target}.exe`,
            "src-tauri/resources/runtime-seed/manifest.json",
          ]
        : []),
    ],
    overlay: x64
      ? "src-tauri/tauri.runtime-seed.conf.json"
      : "src-tauri/tauri.windows-x86.conf.json",
  }
}

export function fileDigest(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex")
}

function collectFiles(root) {
  const files = []
  const pending = [root]
  while (pending.length) {
    const directory = pending.pop()
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name)
      if (entry.isSymbolicLink())
        throw new Error(`staging links are not allowed: ${path}`)
      if (entry.isDirectory()) pending.push(path)
      else if (entry.isFile()) files.push(path)
    }
  }
  return files
}

export function createWindowsStaging({
  root,
  directory,
  target,
  sourceCommit,
}) {
  const layout = windowsLayout(target)
  if (!existsSync(join(root, "out/index.html")))
    throw new Error("current frontend output is missing")
  if (existsSync(directory) && readdirSync(directory).length)
    throw new Error("staging output must be empty")
  mkdirSync(directory, { recursive: true })
  for (const path of [...layout.directories, ...layout.files]) {
    const source = join(root, path)
    if (lstatSync(source).isDirectory()) collectFiles(source)
    else if (lstatSync(source).isSymbolicLink())
      throw new Error("staging source cannot be a link")
    mkdirSync(join(directory, path, ".."), { recursive: true })
    cpSync(source, join(directory, path), { recursive: true })
  }
  const files = collectFiles(directory).map((path) => ({
    path: relative(directory, path).split(sep).join("/"),
    size: lstatSync(path).size,
    sha256: fileDigest(path),
  }))
  const version = JSON.parse(
    readFileSync(join(root, "package.json"), "utf8")
  ).version
  const manifest = { schemaVersion: 1, version, sourceCommit, target, files }
  validateWindowsManifest(manifest, { version, sourceCommit, target })
  writeFileSync(
    join(directory, "staging-manifest.json"),
    JSON.stringify(manifest)
  )
  return manifest
}

function validateEntries(files, layout) {
  const paths = new Set()
  for (const entry of files) {
    const path = entry?.path
    if (
      typeof path !== "string" ||
      isAbsolute(path) ||
      path.includes("\\") ||
      path.includes(":") ||
      path.split("/").some((part) => !part || part === "." || part === "..") ||
      paths.has(path)
    ) {
      throw new Error("unsafe or duplicate staging path")
    }
    if (
      !layout.files.includes(path) &&
      !layout.directories.some((prefix) => path.startsWith(`${prefix}/`))
    ) {
      throw new Error(`unexpected staging file: ${path}`)
    }
    if (
      !Number.isSafeInteger(entry.size) ||
      entry.size < 0 ||
      !/^[a-f0-9]{64}$/.test(entry.sha256)
    ) {
      throw new Error(`invalid staging digest metadata: ${path}`)
    }
    paths.add(path)
  }
  return paths
}

export function validateWindowsManifest(manifest, expected) {
  const layout = windowsLayout(expected.target)
  if (
    manifest.schemaVersion !== 1 ||
    manifest.version !== expected.version ||
    manifest.sourceCommit !== expected.sourceCommit ||
    manifest.target !== expected.target
  ) {
    throw new Error("Windows staging source, version or target mismatch")
  }
  if (!Array.isArray(manifest.files) || !manifest.files.length)
    throw new Error("staging manifest has no files")
  const paths = validateEntries(manifest.files, layout)
  const includesFrontend = [...paths].some((path) => path.startsWith("out/"))
  for (const path of [
    ...layout.required,
    ...(includesFrontend ? ["out/index.html"] : []),
  ]) {
    if (!paths.has(path))
      throw new Error(`required staging file missing: ${path}`)
  }
  return { version: manifest.version, includesFrontend }
}

export function verifyWindowsStaging(directory, expected) {
  const manifest = JSON.parse(
    readFileSync(join(directory, "staging-manifest.json"), "utf8")
  )
  const result = validateWindowsManifest(manifest, expected)
  const root = realpathSync(directory)
  for (const entry of manifest.files) {
    const path = resolve(root, entry.path)
    if (
      !realpathSync(path).startsWith(`${root}${sep}`) ||
      !lstatSync(path).isFile() ||
      lstatSync(path).size !== entry.size ||
      fileDigest(path) !== entry.sha256
    ) {
      throw new Error(`staged file changed: ${entry.path}`)
    }
  }
  const declared = new Set(manifest.files.map((entry) => entry.path))
  for (const path of collectFiles(root)) {
    const name = relative(root, path).split(sep).join("/")
    if (name !== "staging-manifest.json" && !declared.has(name))
      throw new Error(`undeclared staging file: ${name}`)
  }
  return result
}
