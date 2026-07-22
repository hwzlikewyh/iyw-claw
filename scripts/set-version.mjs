import { readFileSync, writeFileSync } from "node:fs"
import { resolve } from "node:path"
import { fileURLToPath } from "node:url"

const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/
const ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)))

function read(path) {
  return readFileSync(resolve(ROOT, path), "utf8")
}

function write(path, content) {
  writeFileSync(resolve(ROOT, path), content, "utf8")
}

function jsonWithVersion(path, version) {
  const source = read(path)
  const data = JSON.parse(source)
  if (typeof data.version !== "string") {
    throw new Error(`${path} does not contain a string version field`)
  }

  const matches = source.match(/^  "version": "[^"]+"/gm) ?? []
  if (matches.length !== 1) {
    throw new Error(
      `${path} expected one root version field, found ${matches.length}`
    )
  }

  return source.replace(/^  "version": "[^"]+"/m, `  "version": "${version}"`)
}

function tauriConfigWithVersion(path, version) {
  const source = jsonWithVersion(path, version)
  const pattern =
    /"binaries\/iyw-claw-mcp-(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)"/g
  const matches = [...source.matchAll(pattern)]
  if (matches.length !== 1) {
    throw new Error(
      `${path} expected one versioned iyw-claw-mcp externalBin, found ${matches.length}`
    )
  }
  return source.replace(pattern, `"binaries/iyw-claw-mcp-${version}"`)
}

function replaceVersionLine(block, path, version) {
  const matches = block.match(/^version = "[^"]+"\r?$/gm) ?? []
  if (matches.length !== 1) {
    throw new Error(
      `${path} expected one version in the iyw-claw package, found ${matches.length}`
    )
  }

  return block.replace(/^version = "[^"]+"/m, `version = "${version}"`)
}

function cargoManifestWithVersion(path, version) {
  const source = read(path)
  const sections = source.split(/(?=^\[[^\]\r\n]+\]\r?$)/m)
  const packageSections = sections.filter((section) =>
    /^\[package\]\r?$/m.test(section)
  )
  if (packageSections.length !== 1) {
    throw new Error(
      `${path} expected one [package] section, found ${packageSections.length}`
    )
  }

  const index = sections.indexOf(packageSections[0])
  sections[index] = replaceVersionLine(sections[index], path, version)
  return sections.join("")
}

function cargoLockWithVersion(path, version) {
  const source = read(path)
  const packages = source.split(/(?=^\[\[package\]\]\r?$)/m)
  const appPackages = packages.filter((block) =>
    /^name = "iyw-claw"\r?$/m.test(block)
  )
  if (appPackages.length !== 1) {
    throw new Error(
      `${path} expected one iyw-claw package, found ${appPackages.length}`
    )
  }

  const index = packages.indexOf(appPackages[0])
  packages[index] = replaceVersionLine(packages[index], path, version)
  return packages.join("")
}

const version = process.argv[2]
if (!version || !SEMVER_PATTERN.test(version)) {
  console.error("Usage: pnpm version:set <major.minor.patch[-prerelease]>")
  process.exit(1)
}

const updates = [
  ["package.json", jsonWithVersion("package.json", version)],
  [
    "src-tauri/tauri.conf.json",
    tauriConfigWithVersion("src-tauri/tauri.conf.json", version),
  ],
  [
    "src-tauri/Cargo.toml",
    cargoManifestWithVersion("src-tauri/Cargo.toml", version),
  ],
  [
    "src-tauri/Cargo.lock",
    cargoLockWithVersion("src-tauri/Cargo.lock", version),
  ],
]

for (const [path, content] of updates) {
  write(path, content)
}
console.log(`Synchronized iyw-claw version to ${version}`)
