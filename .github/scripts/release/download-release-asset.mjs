import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import {
  createReadStream,
  existsSync,
  mkdirSync,
  renameSync,
  rmSync,
  statSync,
} from "node:fs"
import { basename, join } from "node:path"
import { setTimeout as delay } from "node:timers/promises"

const ATTEMPTS = 3
const TRANSFER_SECONDS = 300
const CONNECT_SECONDS = 20
const STALL_SECONDS = 45
const MIN_BYTES_PER_SECOND = 16 * 1024
const RETRY_DELAY_MS = 5000
const PROCESS_GRACE_MS = 5000
const RANGE_ERROR = 33

export async function sha256(path) {
  const hash = createHash("sha256")
  for await (const chunk of createReadStream(path)) hash.update(chunk)
  return hash.digest("hex")
}

async function verified(path, asset) {
  return (
    existsSync(path) &&
    statSync(path).size === asset.size &&
    `sha256:${await sha256(path)}` === asset.digest
  )
}

function configValue(value) {
  if (/[\r\n\0]/.test(value)) throw new Error("invalid download configuration")
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`
}

function connection() {
  const proxy =
    process.env.IYW_CLAW_RELEASE_DOWNLOAD_PROXY ||
    process.env.HTTPS_PROXY ||
    process.env.https_proxy ||
    process.env.HTTP_PROXY ||
    process.env.http_proxy ||
    ""
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN
  const lines = token
    ? [`header = ${configValue(`Authorization: Bearer ${token}`)}`]
    : []
  if (proxy) {
    if (
      !["http:", "https:", "socks5:", "socks5h:"].includes(
        new URL(proxy).protocol
      )
    )
      throw new Error("unsupported release download proxy")
    // 显式覆盖旧 NO_PROXY，防止资产域名绕过已配置的代理。
    lines.push(`proxy = ${configValue(proxy)}`, 'noproxy = ""')
  }
  return { input: `${lines.join("\n")}\n`, route: proxy ? "proxy" : "direct" }
}

const CURL_ARGUMENTS = [
  "--disable",
  "--config",
  "-",
  "--fail",
  "--location",
  "--proto",
  "=https",
  "--proto-redir",
  "=https",
  "--silent",
  "--show-error",
  "--connect-timeout",
  String(CONNECT_SECONDS),
  "--max-time",
  String(TRANSFER_SECONDS),
  "--speed-time",
  String(STALL_SECONDS),
  "--speed-limit",
  String(MIN_BYTES_PER_SECOND),
  "--continue-at",
  "-",
  "--header",
  "Accept: application/octet-stream",
  "--header",
  "X-GitHub-Api-Version: 2022-11-28",
  "--write-out",
  "[release-download] http=%{http_code} bytes=%{size_download} speed=%{speed_download}B/s time=%{time_total}s\n",
]

function transfer({ repo, asset, partial, input }) {
  execFileSync(
    process.platform === "win32" ? "curl.exe" : "curl",
    [
      ...CURL_ARGUMENTS,
      "--output",
      partial,
      `https://api.github.com/repos/${repo}/releases/assets/${asset.id}`,
    ],
    {
      input,
      stdio: ["pipe", "inherit", "inherit"],
      windowsHide: true,
      timeout: TRANSFER_SECONDS * 1000 + PROCESS_GRACE_MS,
    }
  )
}

async function downloadWithRetry(options) {
  const { asset, partial } = options
  const { input, route } = connection()
  for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
    const offset = existsSync(partial) ? statSync(partial).size : 0
    console.log(
      `[release-download] ${asset.name}: ${route}, attempt ${attempt}/${ATTEMPTS}, resume ${offset}/${asset.size} bytes`
    )
    let failure
    try {
      transfer({ ...options, input })
    } catch (error) {
      failure = error.code || error.status || "unknown"
      if (error.status === RANGE_ERROR) rmSync(partial, { force: true })
    }
    if (await verified(partial, asset)) return
    if (existsSync(partial) && statSync(partial).size >= asset.size) {
      rmSync(partial, { force: true })
      failure = "size or SHA-256 mismatch"
    }
    if (attempt === ATTEMPTS)
      throw new Error(
        `release download failed: ${asset.name} (${route}, ${failure || "incomplete transfer"}); partial retained for retry`
      )
    console.warn(
      `[release-download] ${asset.name}: ${failure || "incomplete transfer"}; retrying with partial download`
    )
    await delay(RETRY_DELAY_MS * attempt)
  }
}

export async function downloadReleaseFile(context, asset, directory) {
  const validNumbers = [asset.id, asset.size].every(
    (value) => Number.isSafeInteger(value) && value > 0
  )
  if (
    !/^[\w.-]+\/[\w.-]+$/.test(context.repo) ||
    !validNumbers ||
    !asset.name ||
    basename(asset.name) !== asset.name ||
    /[\\/:]/.test(asset.name) ||
    !/^sha256:[a-f0-9]{64}$/.test(asset.digest)
  )
    throw new Error("invalid GitHub asset identity, size or digest")
  mkdirSync(directory, { recursive: true })
  const path = join(directory, asset.name)
  if (await verified(path, asset)) {
    console.log(`[release-download] verified local cache: ${asset.name}`)
    return path
  }
  // Asset ID 不可变；不同版本的残留片段不能混用。
  const partial = join(directory, `${asset.id}.part`)
  if (!(await verified(partial, asset)))
    await downloadWithRetry({ repo: context.repo, asset, partial })
  renameSync(partial, path)
  console.log(`[release-download] verified ${asset.size} bytes: ${asset.name}`)
  return path
}
