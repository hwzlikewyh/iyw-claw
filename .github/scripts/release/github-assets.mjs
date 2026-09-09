import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { createReadStream, existsSync, mkdirSync, statSync } from "node:fs"
import { basename, join } from "node:path"

const COMMAND_TIMEOUT = 20 * 60 * 1000

export function gh(args, options = {}) {
  return execFileSync("gh", args, {
    encoding: "utf8",
    windowsHide: true,
    timeout: COMMAND_TIMEOUT,
    maxBuffer: 8 * 1024 * 1024,
    ...options,
  })
}

export function github(path, args = []) {
  const text = gh(["api", path, ...args])
  return text.trim() ? JSON.parse(text) : null
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
  gh(
    [
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
    ],
    { stdio: "inherit" }
  )
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
