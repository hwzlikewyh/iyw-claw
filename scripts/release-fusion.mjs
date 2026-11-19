import { createReadStream, readFileSync, statSync } from "node:fs"
import { connectFusion } from "./release-fusion-client.mjs"
import { submitArtifactReceipt } from "./release-fusion-receipts.mjs"
export { connectFusion } from "./release-fusion-client.mjs"
import {
  github,
  downloadReleaseFile,
  sha256,
} from "../.github/scripts/release/github-assets.mjs"

const UPLOAD_TIMEOUT = 20 * 60 * 1000
const ROLLOUT_PERCENT = 1
const UPLOAD_CONCURRENCY = 2
// 与正式发布矩阵一致，所有目标均上传已签名的专用制品。
const PLATFORMS = [
  ["windows", "x86_64", "nsis", "x64-setup.exe"],
  ["windows", "x86_64", "app_payload_zip", "x64-app-payload.zip"],
  ["windows", "x86_64", "bootstrapper", "x64-bootstrapper.exe"],
  ["windows7", "x86_64", "nsis", "win7-x64-setup.exe"],
  ["windows7", "x86_64", "bootstrapper", "win7-x64-bootstrapper.exe"],
  ["windows7", "i686", "nsis", "win7-x86-setup.exe"],
  ["windows7", "i686", "bootstrapper", "win7-x86-bootstrapper.exe"],
  ["darwin", "x86_64", "app_tar_gz", "x64.app.tar.gz"],
  ["darwin", "aarch64", "app_tar_gz", "aarch64.app.tar.gz"],
  ["linux", "x86_64", "appimage", "amd64.AppImage"],
  ["linux", "aarch64", "appimage", "arm64.AppImage"],
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
    const ending =
      target === "darwin"
        ? `_${suffix}`
        : `_${release.tag_name.slice(1)}_${suffix}`
    const matches = release.assets.filter((asset) =>
      asset.name.endsWith(ending)
    )
    if (!matches.length && target === "windows7")
      continue
    if (matches.length !== 1)
      throw new Error(`signed release asset missing or ambiguous: ${ending}`)
    const name = matches[0].name
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
  const endpoint = new URL(upload.url)
  if (endpoint.protocol !== "https:" || !/^[a-z0-9-]+\.tos-[a-z0-9-]+\.volces\.com$/.test(endpoint.hostname) ||
      endpoint.username || endpoint.password || endpoint.port || upload.method !== "PUT")
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
  const completed = await submitArtifactReceipt(api, found, input)
  if (completed) sameArtifact(completed, input)
  else console.log(`[release] waiting for verifier receipt: ${found.releaseId}/${found.id} ${input.fileName}`)
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
  if (found?.status === "uploading") return completePreviousUpload(api, input, found)
  const { path, ...payload } = input
  const initialized = await api(
    `/admin/api/app-releases/${releaseId}/artifacts/init`,
    payload
  )
  console.log(`[release] uploading ${input.fileName}`)
  await sendObject(initialized.upload, input)
  await completePreviousUpload(api, input, initialized.artifact)
}

async function uploadArtifacts(api, releaseId, inputs) {
  let next = 0
  let failed = false
  const workers = Array.from({ length: UPLOAD_CONCURRENCY }, async () => {
    while (!failed && next < inputs.length) {
      const input = inputs[next++]
      try {
        await uploadArtifact(api, releaseId, input)
      } catch (error) {
        failed = true
        throw error
      }
    }
  })
  // 等待已开始的另一份上传完成，再报告失败；重试只补未通过校验的制品。
  const results = await Promise.allSettled(workers)
  const failure = results.find(result => result.status === "rejected")
  if (failure) throw failure.reason
}

async function verifyUpdateCatalog(api, release, inputs) {
  for (const input of inputs) {
    const route = `/app-updates/v1/check/${input.target}/${input.arch}`
    const update = await api(`${route}/0.1.92?reason=manual`)
    const artifact = input.packageKind === "app_payload_zip" ? update?.app_payload :
      input.packageKind === "bootstrapper" ? update?.bootstrapper : update
    if (
      !update ||
      update.version !== release.version ||
      update.release_id !== release.id ||
      update.rollout_percent !== ROLLOUT_PERCENT ||
      artifact?.sha256 !== input.sha256 ||
      artifact?.size !== input.size
    ) {
      throw new Error(
        `published release update check failed: ${input.target}/${input.arch}`
      )
    }
    const current = await api(`${route}/${release.version}?reason=manual`)
    if (current && Object.keys(current).length > 0)
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
  await uploadArtifacts(api, value.id, inputs)
  const path = `/admin/api/app-releases/${value.id}`
  const ready = await api(path)
  assertPolicy(ready.release, version)
  const pending = ready.artifacts.filter(item => item.status !== "ready")
  if (pending.length) throw new Error(`等待独立验签回执，当前草稿 ${value.id} 尚有 ${pending.length} 件未就绪。可用相同版本重试。`)
  for (const input of inputs)
    sameArtifact(
      ready.artifacts.find(
        (item) => item.target === input.target && item.arch === input.arch && item.packageKind === input.packageKind
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
