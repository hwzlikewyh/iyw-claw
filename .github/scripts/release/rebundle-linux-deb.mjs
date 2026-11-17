import { execFileSync } from "node:child_process"
import { chmodSync, closeSync, copyFileSync, cpSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { join, resolve, sep } from "node:path"
import { downloadReleaseFile, requireReleaseSource, sha256, uploadDraftFile } from "./github-assets.mjs"
import { verifyMinisign } from "./verify-minisign.mjs"
import { readDebControl } from "./linux-deb-control.mjs"

const target = "aarch64-unknown-linux-gnu"
const version = process.env.VERSION
const context = { repo: process.env.GITHUB_REPOSITORY, releaseId: process.env.RELEASE_ID,
  tag: `v${version}`, sourceRef: process.env.SOURCE_REF }
if (!/^\d+\.\d+\.\d+$/.test(version) || !/^[a-f0-9]{40}$/.test(context.sourceRef))
  throw new Error("expected immutable release source and version")
const directory = join(process.env.RUNNER_TEMP, `rebundle-${process.env.GITHUB_RUN_ID}`)
mkdirSync(directory, { recursive: true })
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"))
const publicKey = Buffer.from(config.plugins.updater.pubkey, "base64").toString("utf8")
  .split(/\r?\n/).find(line => line.startsWith("RW"))
if (!publicKey) throw new Error("updater public key missing")

async function verify(path) {
  await verifyMinisign(path, readFileSync(`${path}.sig`, "utf8"), publicKey)
}

async function codeDigest(binary) {
  const code = join(directory, "application.text")
  execFileSync("objcopy", ["--dump-section", `.text=${code}`, binary, "/dev/null"], { stdio: "inherit" })
  return sha256(code)
}

async function restore() {
  const release = requireReleaseSource(context)
  const name = `iyw-claw_${version}_arm64.deb`
  for (const fileName of [name, `${name}.sig`]) {
    const assets = release.assets.filter(asset => asset.name === fileName)
    if (assets.length !== 1) throw new Error(`signed source DEB missing: ${fileName}`)
    await downloadReleaseFile(context, assets[0], directory)
  }
  const deb = join(directory, name)
  await verify(deb)
  const control = readDebControl(deb)
  const field = name => control.match(new RegExp(`^${name}:\\s*(.+)$`, "m"))?.[1].trim()
  if (field("Version") !== version || field("Architecture") !== "arm64") throw new Error("DEB platform or version mismatch")
  const extracted = join(directory, "extracted")
  execFileSync("dpkg-deb", ["--extract", deb, extracted], { stdio: "inherit" })
  const releaseDir = join("src-tauri/target", target, "release")
  mkdirSync(releaseDir, { recursive: true })
  const binary = join(extracted, "usr/bin/iyw-claw")
  const header = execFileSync("readelf", ["-h", binary], { encoding: "utf8", env: { ...process.env, LC_ALL: "C" } })
  if (!/Machine:\s+AArch64/.test(header)) throw new Error("DEB binary architecture mismatch")
  copyFileSync(binary, join(releaseDir, "iyw-claw"))
  chmodSync(join(releaseDir, "iyw-claw"), 0o755)
  mkdirSync("src-tauri/binaries", { recursive: true })
  copyFileSync(join(extracted, "usr/bin/iyw-environment"), `src-tauri/binaries/iyw-environment-${target}`)
  const lib = join(extracted, "usr/lib")
  const roots = readdirSync(lib).map(name => join(lib, name)).filter(path => existsSync(join(path, "resources/bundle")))
  if (roots.length !== 1) throw new Error("DEB resources root is ambiguous")
  for (const [source, bundled] of Object.entries(config.bundle.resources)) {
    const destination = resolve("src-tauri", source)
    if (!destination.startsWith(`${resolve(".")}${sep}`)) throw new Error("resource destination escapes workspace")
    cpSync(join(roots[0], bundled), destination, { recursive: true })
  }
  const digest = await sha256(binary)
  writeFileSync(join(directory, "application-code.sha256"), await codeDigest(binary))
  console.log(`[rebundle] verified existing application; sha256=${digest}; source=${context.sourceRef}`)
}

async function verifyContents(path, kind, directory) {
  const extracted = join(directory, `verified-${kind}`)
  mkdirSync(extracted, { recursive: true })
  if (kind === "deb") {
    execFileSync("dpkg-deb", ["--extract", path, extracted], { stdio: "inherit" })
    const name = execFileSync("dpkg-deb", ["--field", path, "Package"], { encoding: "utf8" }).trim()
    if (name !== "iyw-claw") throw new Error("invalid DEB package identity")
  } else if (kind === "appimage") {
    chmodSync(path, 0o755)
    execFileSync(resolve(path), ["--appimage-extract"], { cwd: extracted, stdio: "ignore" })
  } else {
    const archive = join(extracted, "payload.cpio")
    const output = openSync(archive, "w")
    try { execFileSync("rpm2cpio", [path], { stdio: ["ignore", output, "inherit"] }) }
    finally { closeSync(output) }
    const input = openSync(archive, "r")
    try { execFileSync("cpio", ["-id", "--quiet"], { cwd: extracted, stdio: [input, "ignore", "inherit"] }) }
    finally { closeSync(input) }
  }
  const binary = kind === "appimage" ? join(extracted, "squashfs-root/usr/bin/iyw-claw") : join(extracted, "usr/bin/iyw-claw")
  // Tauri 写入包类型标记、linuxdeploy 改写 RPATH，验证代码段可排除这些正常打包变动。
  const original = readFileSync(join(directory, "application-code.sha256"), "utf8")
  if (await codeDigest(binary) !== original) throw new Error(`${kind} changed the verified application code`)
}

async function publish() {
  requireReleaseSource(context)
  const root = join("src-tauri/target", target, "release/bundle")
  for (const [kind, suffix] of [["deb", "deb"], ["appimage", "AppImage"], ["rpm", "rpm"]]) {
    const files = readdirSync(join(root, kind)).filter(name => name.endsWith(`.${suffix}`))
    if (files.length !== 1) throw new Error(`expected one ${kind}`)
    const output = join(directory, `iyw-claw_${version}_arm64.${suffix}`)
    copyFileSync(join(root, kind, files[0]), output)
    execFileSync(process.execPath, ["node_modules/@tauri-apps/cli/tauri.js", "signer", "sign", output], { stdio: "inherit" })
    await verify(output)
    await verifyContents(output, kind, directory)
    await uploadDraftFile(context, output)
    await uploadDraftFile(context, `${output}.sig`)
  }
}

if (process.argv[2] === "restore") await restore()
else if (process.argv[2] === "publish") await publish()
else throw new Error("expected restore or publish")
