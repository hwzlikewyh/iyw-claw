import {
  appendFileSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  readdirSync,
} from "node:fs"
import { execFileSync } from "node:child_process"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import {
  windowsLayout,
  createWindowsStaging,
} from "../../../src-tauri/scripts/windows-staging.mjs"
import {
  downloadReleaseFile,
  requireDraft,
  uploadDraftFile,
} from "./github-assets.mjs"

const ROOT = process.cwd()
const TARGET = process.env.TAURI_TARGET_TRIPLE
const TOOL_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..")
const layout = windowsLayout(TARGET)
const context = {
  repo: process.env.GITHUB_REPOSITORY,
  releaseId: process.env.RELEASE_ID,
  tag: process.env.RELEASE_TAG,
}
const source = () =>
  execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
    windowsHide: true,
  }).trim()
const version = () =>
  JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8")).version

function output(values) {
  for (const [key, value] of Object.entries(values))
    appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${value}\n`)
}

function node(script, args = [], environment = process.env) {
  execFileSync(process.execPath, [script, ...args], {
    cwd: ROOT,
    env: environment,
    stdio: "inherit",
    windowsHide: true,
  })
}

async function prepare() {
  if (source() !== process.env.SOURCE_REF || `v${version()}` !== context.tag)
    throw new Error("build source or version mismatch")
  createWindowsStaging({
    root: ROOT,
    directory: process.env.IYW_CLAW_STAGING_DIR,
    target: TARGET,
    sourceCommit: source(),
  })
}

async function upload() {
  const asset = await uploadDraftFile(context, process.env.STAGING_ARCHIVE)
  output({
    asset_id: asset.id,
    sha256: asset.digest.slice("sha256:".length),
    name: asset.name,
  })
}

async function download() {
  const release = requireDraft(context)
  const asset = release.assets.find(
    (value) => String(value.id) === process.env.STAGING_ASSET_ID
  )
  if (!asset || asset.digest !== `sha256:${process.env.STAGING_SHA256}`)
    throw new Error("staging asset changed since build")
  const directory = join(
    process.env.RUNNER_TOOL_CACHE,
    "iyw-staging",
    process.env.STAGING_SHA256
  )
  const path = await downloadReleaseFile(context, asset, directory)
  output({ archive: path })
}

async function finalize() {
  if (source() !== process.env.SOURCE_REF || `v${version()}` !== context.tag)
    throw new Error("signing source or version mismatch")
  node(join(TOOL_ROOT, "src-tauri/scripts/finalize-staged-windows.mjs"), [], {
    ...process.env,
    IYW_CLAW_BUILD_ROOT: ROOT,
  })
  const directory = join(
    ROOT,
    "src-tauri/target",
    TARGET,
    "release/bundle/nsis"
  )
  const packages = readdirSync(directory).filter((name) =>
    name.endsWith(`_${version()}_${layout.arch}-setup.exe`)
  )
  if (packages.length !== 1)
    throw new Error("expected one signed Windows installer")
  const installer = join(directory, packages[0])
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY)
    throw new Error("updater signing key is missing")
  node(join(ROOT, "node_modules/@tauri-apps/cli/tauri.js"), [
    "signer",
    "sign",
    installer,
  ])
  node(join(ROOT, "src-tauri/scripts/verify-xinghe-worker-bundle.mjs"), [
    "--target",
    TARGET,
  ])
  node(join(ROOT, "src-tauri/scripts/verify-desktop-bundle-size.mjs"), [
    "--target",
    TARGET,
  ])
  const assets = join(process.env.RUNNER_TEMP, `release-assets-${TARGET}`)
  mkdirSync(assets, { recursive: true })
  const name = `iyw-claw_${version()}_${layout.arch}-setup.exe`
  copyFileSync(installer, join(assets, name))
  copyFileSync(`${installer}.sig`, join(assets, `${name}.sig`))
  await uploadDraftFile(context, join(assets, name))
  await uploadDraftFile(context, join(assets, `${name}.sig`))
}

const commands = { prepare, upload, download, finalize }
try {
  const command = commands[process.argv[2]]
  if (!command)
    throw new Error("expected prepare, upload, download or finalize")
  await command()
} catch (error) {
  console.error(`[windows-release] ${error.message}`)
  process.exitCode = 1
}
