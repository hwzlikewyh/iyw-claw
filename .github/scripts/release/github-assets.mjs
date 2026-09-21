import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { createReadStream, existsSync, mkdirSync, statSync } from "node:fs"
import { basename, join } from "node:path"
import { setTimeout as delay } from "node:timers/promises"

const COMMAND_TIMEOUT = 20 * 60 * 1000
const DOWNLOAD_TIMEOUT = 5 * 60 * 1000
const DOWNLOAD_ATTEMPTS = 3
const DOWNLOAD_RETRY_DELAY = 5000

export function gh(args, options = {}) {
  return execFileSync("gh", args, {
    encoding: "utf8",
    windowsHide: true,
    timeout: COMMAND_TIMEOUT,
    maxBuffer: 8 * 1024 * 1024,
    // A Windows runner's console enables coloured output, so `gh` wrapped the
    // JSON in ANSI escapes and every parse died on the leading ESC byte with
    // `Unexpected token '\x1b', "\x1b[1;37m{[\x1b[m..."`. Disable colour,
    // paging and prompts for the child so its stdout is always plain JSON.
    env: {
      ...process.env,
      NO_COLOR: "1",
      CLICOLOR: "0",
      CLICOLOR_FORCE: "0",
      GH_PAGER: "cat",
      GH_FORCE_TTY: "0",
      GITHUB_ACTIONS: "1",
      TERM: "dumb",
    },
    ...options,
  })
}

export function github(path, args = []) {
  const text = gh(["api", path, ...args])
  // `gh` on a Windows runner can emit a UTF-8 BOM. `String.trim()` does not
  // strip U+FEFF, so JSON.parse used to fail with
  // `Unexpected token '\ufeff', "{"... is not valid JSON` on the very first
  // character. Strip it before parsing and keep the raw output in the error so
  // an unexpected body is diagnosable.
  const cleaned = text.replace(/^\uFEFF/, "")
  if (!cleaned.trim()) return null
  try {
    return JSON.parse(cleaned)
  } catch (error) {
    throw new Error(
      `gh api ${path} returned invalid JSON (${error.message}); ` +
        `first 200 chars: ${cleaned.slice(0, 200)}`
    )
  }
}

export async function sha256(path) {
  const hash = createHash("sha256")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest("hex")
}

export function requireDraft({ repo, releaseId, tag }) {
  const release = github(`repos/${repo}/releases/${releaseId}`)
  if (!release.draft || release.tag_name !== tag)
    throw new Error("expected matching GitHub draft release")
  return release
}

export async function uploadDraftFile(context, path) {
  const release = requireDraft(context)
  const name = basename(path)
  const size = statSync(path).size
  const digest = `sha256:${await sha256(path)}`
  const existing = release.assets.filter((asset) => asset.name === name)
  if (existing.length > 1) throw new Error(`duplicate release asset: ${name}`)
  if (existing[0]?.digest === digest && existing[0].size === size)
    return existing[0]
  if (existing[0])
    github(`repos/${context.repo}/releases/assets/${existing[0].id}`, [
      "--method",
      "DELETE",
    ])
  const endpoint = `https://uploads.github.com/repos/${context.repo}/releases/${context.releaseId}/assets?name=${encodeURIComponent(name)}`
  const asset = github(endpoint, [
    "--method",
    "POST",
    "--header",
    "Content-Type: application/octet-stream",
    "--input",
    path,
  ])
  if (
    asset.state !== "uploaded" ||
    asset.digest !== digest ||
    asset.size !== size
  ) {
    throw new Error(`uploaded GitHub asset does not match local bytes: ${name}`)
  }
  return asset
}

export async function downloadReleaseFile(context, asset, directory) {
  if (
    basename(asset.name) !== asset.name ||
    /[\\/:]/.test(asset.name) ||
    !/^sha256:[a-f0-9]{64}$/.test(asset.digest || "")
  ) {
    throw new Error("invalid GitHub artifact name or digest")
  }
  mkdirSync(directory, { recursive: true })
  const path = join(directory, asset.name)
  if (
    existsSync(path) &&
    statSync(path).size === asset.size &&
    `sha256:${await sha256(path)}` === asset.digest
  )
    return path
  await downloadWithRetry(context, asset, directory)
  if (
    statSync(path).size !== asset.size ||
    `sha256:${await sha256(path)}` !== asset.digest
  ) {
    throw new Error(
      `downloaded GitHub asset failed verification: ${asset.name}`
    )
  }
  return path
}

async function downloadWithRetry(context, asset, directory) {
  const path = join(directory, asset.name)
  const args = [
    "release",
    "download",
    context.tag,
    "--repo",
    context.repo,
    "--pattern",
    asset.name,
    "--dir",
    directory,
    "--clobber",
  ]
  for (let attempt = 1; attempt <= DOWNLOAD_ATTEMPTS; attempt += 1) {
    try {
      gh(args, { stdio: "inherit", timeout: DOWNLOAD_TIMEOUT })
      return
    } catch (error) {
      // 下载完成后进程异常退出时，仍以制品摘要为准。
      if (
        existsSync(path) &&
        statSync(path).size === asset.size &&
        `sha256:${await sha256(path)}` === asset.digest
      )
        return
      if (attempt === DOWNLOAD_ATTEMPTS) throw error
      console.warn(
        `[release-download] ${asset.name}: attempt ${attempt}/${DOWNLOAD_ATTEMPTS} failed (${error.code || error.status || "unknown"}); retrying`
      )
      await delay(DOWNLOAD_RETRY_DELAY * attempt)
    }
  }
}

export function cleanupStaging(context) {
  const release = requireDraft(context)
  for (const asset of release.assets) {
    if (
      /^iyw-staging-\d+-\d+-(x86_64|i686)-pc-windows-msvc\.zip$/.test(
        asset.name
      )
    ) {
      github(`repos/${context.repo}/releases/assets/${asset.id}`, [
        "--method",
        "DELETE",
      ])
    }
  }
}
