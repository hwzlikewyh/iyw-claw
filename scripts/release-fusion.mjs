import { createReadStream, readFileSync, statSync } from "node:fs"
import { connectFusion } from "./release-fusion-client.mjs"
export { connectFusion } from "./release-fusion-client.mjs"
import {
  github,
  downloadReleaseFile,
  sha256,
} from "../.github/scripts/release/github-assets.mjs"

const UPLOAD_TIMEOUT = 20 * 60 * 1000
const ROLLOUT_PERCENT = 1
const PLATFORMS = [
  ["windows", "x86_64", "nsis", "x64-setup.exe"],
  ["windows", "i686", "nsis", "x86-setup.exe"],
  ["linux", "x86_64", "appimage", "amd64.AppImage"],
  ["darwin", "x86_64", "app_tar_gz", "x64.app.tar.gz"],
  ["darwin", "aarch64", "app_tar_gz", "aarch64.app.tar.gz"],
]

export function releasePolicy(version, notes) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version))
    throw new Error("version must be a stable x.y.z version")
  const [major, minor, patch] = version.split(".").map(Number)
  if (major === 0 && (minor < 1 || (minor === 1 && patch < 93)))
    throw new Error("desktop releases require version 0.1.93 or newer")
  return {
    productKey: "iyw-claw",
    version,
    channel: "stable",
    title: `原助理 ${version}`,
    notesMarkdown: notes,
    updatePolicy: "optional",
    rolloutMode: "manual",
    rolloutPercent: ROLLOUT_PERCENT,
    enforceAfter: null,
    requiredBelowVersion: "",
  }
}

export function assertPolicy(release, version) {
  if (
    release.productKey !== "iyw-claw" ||
    release.version !== version ||
    release.channel !== "stable" ||
    release.updatePolicy !== "optional" ||
    release.rolloutMode !== "manual" ||
    release.rolloutBasisPoints !== 100 ||
    release.enforceAfter ||
    release.requiredBelowVersion
  ) {
    throw new Error(
      "existing Fusion release does not use the requested stable / optional / manual 1% policy"
    )
  }
}

export function normalizeSignature(value) {
  if (typeof value !== "string") throw new Error("Tauri signature is missing")
  const text = Buffer.from(value.trim(), "base64").toString("utf8").trim()
  if (!text.startsWith("untrusted comment:"))
    throw new Error("invalid Tauri signature")
  return Buffer.from(text).toString("base64")
}

async function draft(api, release) {
  const list = await api(
    `/admin/api/app-releases?q=${encodeURIComponent(release.version)}&pageSize=100`
  )
  const matches = list.items.filter(
    (item) =>
      item.version === release.version &&
      item.productKey === release.productKey &&
      item.channel === release.channel
  )
  if (matches.length > 1)
    throw new Error("multiple Fusion releases match the version")
  const value =
    matches[0] || (await api("/admin/api/app-releases", release)).release
  assertPolicy(value, release.version)
  if (!["draft", "published"].includes(value.status))
    throw new Error(`release is ${value.status}; manual review required`)
  return value
}

function assetByName(release, name) {
  const assets = release.assets.filter(
    (value) => value.name === name && value.state === "uploaded"
  )
  if (assets.length !== 1 || assets[0].size <= 0)
    throw new Error(`signed release asset missing or ambiguous: ${name}`)
  return assets[0]
}

async function artifactInputs(context, release, directory) {
  const result = []
  for (const [target, arch, packageKind, suffix] of PLATFORMS) {
    const name = `iyw-claw_${release.tag_name.slice(1)}_${suffix}`
    const asset = assetByName(release, name)
    const signatureAsset = assetByName(release, `${name}.sig`)
    const path = await downloadReleaseFile(context, asset, directory)
    const signaturePath = await downloadReleaseFile(
      context,
      signatureAsset,
      directory
    )
    result.push({
      runtime: "desktop",
      target,
      arch,
      packageKind,
      fileName: name,
      path,
      contentType: "application/octet-stream",
      size: statSync(path).size,
      sha256: await sha256(path),
      signature: normalizeSignature(readFileSync(signaturePath, "utf8")),
      ...(target === "darwin" ? { minOsVersion: "13.5" } : {}),
    })
    console.log(`[release] verified source asset: ${target}/${arch}`)
  }
  return result
}

function sameArtifact(artifact, input) {
  if (
    artifact?.status !== "ready" ||
    artifact.runtime !== input.runtime ||
    artifact.target !== input.target ||
    artifact.arch !== input.arch ||
    artifact.packageKind !== input.packageKind ||
    artifact.sha256 !== input.sha256 ||
    artifact.size !== input.size ||
    normalizeSignature(artifact.signature) !== input.signature ||
    (artifact.minOsVersion || "") !== (input.minOsVersion || "")
  ) {
    throw new Error(
      `Fusion artifact differs from signed source: ${input.fileName}`
    )
  }
}

async function sendObject(upload, input) {
  if (new URL(upload.url).protocol !== "https:" || upload.method !== "PUT")
    throw new Error("invalid object upload ticket")
  const stream = createReadStream(input.path)
  try {
    const response = await fetch(upload.url, {
      method: "PUT",
      headers: { ...upload.headers, "Content-Length": String(input.size) },
      body: stream,
      duplex: "half",
      signal: AbortSignal.timeout(UPLOAD_TIMEOUT),
      redirect: "error",
    })
    if (!response.ok) throw new Error(`object upload HTTP ${response.status}`)
    await response.body?.cancel()
  } catch {
    throw new Error(
      `object upload failed: ${input.fileName}; retry the same version to resume`
    )
  } finally {
    stream.destroy()
  }
}

async function completePreviousUpload(api, input, found) {
  if (
    found.size !== input.size ||
    found.fileName !== input.fileName ||
    normalizeSignature(found.signature) !== input.signature
  )
    throw new Error("in-progress artifact differs from the requested release")
  try {
    const complete = await api(
      `/admin/api/app-releases/${found.releaseId}/artifacts/${found.id}/complete`,
      {}
    )
    sameArtifact(complete.artifact, input)
    return true
  } catch (error) {
    if (
      !["APP_RELEASE_UPLOAD_MISSING", "APP_RELEASE_INTEGRITY_FAILED"].includes(
        error.code
      )
    )
      throw error
  }
  return false
}

async function uploadArtifact(api, releaseId, input) {
  const detail = await api(`/admin/api/app-releases/${releaseId}`)
  const found = detail.artifacts.find(
    (value) =>
      value.target === input.target &&
      value.arch === input.arch &&
      value.packageKind === input.packageKind
  )
  if (found?.status === "ready") {
    sameArtifact(found, input)
    return
  }
  if (detail.release.status !== "draft")
    throw new Error("published Fusion artifacts cannot be changed")
  if (
    found?.status === "uploading" &&
    (await completePreviousUpload(api, input, found))
  )
    return
  const { path, sha256: hash, ...payload } = input
  const initialized = await api(
    `/admin/api/app-releases/${releaseId}/artifacts/init`,
    payload
  )
  console.log(`[release] uploading ${input.fileName}`)
  await sendObject(initialized.upload, input)
  const completed = await api(
    `/admin/api/app-releases/${releaseId}/artifacts/${initialized.artifact.id}/complete`,
    {}
  )
  sameArtifact(completed.artifact, input)
}

async function verifyUpdateCatalog(api, release, inputs) {
  for (const input of inputs) {
    const route = `/app-updates/v1/check/${input.target}/${input.arch}`
    const update = await api(`${route}/0.1.92?reason=manual`)
    if (
      !update ||
      update.version !== release.version ||
      update.release_id !== release.id ||
      update.rollout_percent !== ROLLOUT_PERCENT ||
      update.update_policy !== "optional" ||
      update.sha256 !== input.sha256 ||
      update.size !== input.size
    ) {
      throw new Error(
        `published release update check failed: ${input.target}/${input.arch}`
      )
    }
    if (await api(`${route}/${release.version}?reason=manual`))
      throw new Error("current version unexpectedly receives an update")
  }
}

export async function publishFusion({ repo, version, directory }) {
  releasePolicy(version, "")
  const context = { repo, tag: `v${version}` }
  const release = github(`repos/${repo}/releases/tags/${context.tag}`)
  if (release.draft || release.prerelease)
    throw new Error("GitHub signed release must be published first")
  if (release.assets.some((asset) => asset.name.startsWith("iyw-staging-")))
    throw new Error("temporary staging assets remain in release")
  const api = await connectFusion()
  const inputs = await artifactInputs(context, release, directory)
  const value = await draft(
    api,
    releasePolicy(version, release.body || `原助理 ${version}`)
  )
  for (const input of inputs) await uploadArtifact(api, value.id, input)
  const path = `/admin/api/app-releases/${value.id}`
  const ready = await api(path)
  assertPolicy(ready.release, version)
  for (const input of inputs)
    sameArtifact(
      ready.artifacts.find(
        (item) => item.target === input.target && item.arch === input.arch
      ),
      input
    )
  if (ready.release.status === "draft") await api(`${path}/publish`, {})
  const published = await api(path)
  assertPolicy(published.release, version)
  if (published.release.status !== "published")
    throw new Error("Fusion publish was not confirmed")
  await verifyUpdateCatalog(api, published.release, inputs)
  console.log(
    `[release] Fusion ${version} published: id=${value.id}, optional update, manual rollout=1%`
  )
  return published
}
