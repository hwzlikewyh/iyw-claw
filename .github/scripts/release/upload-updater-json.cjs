const UPDATER_FILENAME = "latest.json"
const LEGACY_MCP_ASSET_PATTERN = /^iyw-claw-mcp(?:-|\.|$)/i
const replaceReleaseAsset = require("./replace-release-asset.cjs")
// Match installers by suffix: the product name is non-ASCII ("原助理") and
// gets sanitized away in uploaded asset names, so never match on the prefix.
const PLATFORM_PATTERNS = [
  { platform: "windows-x86_64", pattern: /x64-setup\.exe$/ },
  { platform: "windows-i686", pattern: /x86-setup\.exe$/ },
  { platform: "darwin-x86_64", pattern: /x64\.app\.tar\.gz$/ },
  { platform: "darwin-aarch64", pattern: /aarch64\.app\.tar\.gz$/ },
  { platform: "linux-x86_64", pattern: /amd64\.AppImage$/ },
]
async function fetchAssetText({ github, owner, repo, assetId }) {
  const response = await github.request(
    "GET /repos/{owner}/{repo}/releases/assets/{asset_id}",
    {
      owner,
      repo,
      asset_id: assetId,
      headers: { accept: "application/octet-stream" },
    }
  )
  return Buffer.from(response.data).toString("utf8").trim()
}

function assertNoLegacyMcpAssets(assets) {
  const forbidden = assets
    .map((asset) => asset.name)
    .filter((name) => LEGACY_MCP_ASSET_PATTERN.test(name))
  if (forbidden.length > 0) {
    throw new Error(
      `Legacy MCP release assets are forbidden: ${forbidden.join(", ")}`
    )
  }
}

function assetReadinessError(asset) {
  if (!asset || typeof asset !== "object") {
    return "metadata is missing"
  }
  if (asset.state !== "uploaded") {
    return `state is ${JSON.stringify(asset.state)}, expected "uploaded"`
  }
  if (!Number.isFinite(asset.size) || asset.size <= 0) {
    return `size is ${JSON.stringify(asset.size)}, expected a positive number`
  }
  return ""
}

function assertAssetReady(asset, label = asset?.name ?? "Asset") {
  const error = assetReadinessError(asset)
  if (error) {
    throw new Error(`${label} is not publishable: ${error}`)
  }
}

function findSingleNamedAsset({ assets, name, required, label = name }) {
  const matches = assets.filter((asset) => asset.name === name)
  const hasValidCount = required ? matches.length === 1 : matches.length <= 1
  if (!hasValidCount) {
    const expectation = required ? "one" : "at most one existing"
    throw new Error(`Expected ${expectation} ${label}, found ${matches.length}`)
  }
  return matches[0]
}

async function listReleaseAssets({ github, owner, repo, releaseId }) {
  return github.paginate(github.rest.repos.listReleaseAssets, {
    owner,
    repo,
    release_id: releaseId,
    per_page: 100,
  })
}

async function resolveUpdaterPlatform({
  github,
  owner,
  repo,
  assets,
  platform,
  pattern,
}) {
  const installers = assets.filter((asset) => pattern.test(asset.name))
  if (installers.length !== 1) {
    return {
      error: `${platform}: expected one installer matching ${pattern}, found ${installers.length}`,
    }
  }
  const installer = installers[0]
  const installerError = assetReadinessError(installer)
  if (installerError) {
    return {
      error: `${platform}: installer ${installer.name} ${installerError}`,
    }
  }
  const signatures = assets.filter(
    (asset) => asset.name === `${installer.name}.sig`
  )
  if (signatures.length !== 1) {
    return {
      error: `${platform}: expected one signature for ${installer.name}, found ${signatures.length}`,
    }
  }
  const signatureAsset = signatures[0]
  const signatureError = assetReadinessError(signatureAsset)
  if (signatureError) {
    return {
      error: `${platform}: signature ${signatureAsset.name} ${signatureError}`,
    }
  }
  const signature = await fetchAssetText({
    github,
    owner,
    repo,
    assetId: signatureAsset.id,
  })
  if (!signature) {
    return { error: `${platform}: signature for ${installer.name} is empty` }
  }
  return { installer, signature }
}

async function resolveUpdaterPlatforms({ github, owner, repo, assets, core }) {
  const platforms = {}
  const errors = []
  for (const { platform, pattern } of PLATFORM_PATTERNS) {
    const resolved = await resolveUpdaterPlatform({
      github,
      owner,
      repo,
      assets,
      platform,
      pattern,
    })
    if (resolved.error) {
      errors.push(resolved.error)
      continue
    }
    platforms[platform] = resolved
    core.info(`${platform}: ${resolved.installer.name}`)
  }
  if (errors.length > 0) {
    throw new Error(
      `Updater release asset validation failed:\n${errors.join("\n")}`
    )
  }
  return platforms
}

function formatUpdaterPlatforms(resolved, releaseDownloadBase) {
  const platforms = {}
  for (const [platform, { installer, signature }] of Object.entries(resolved)) {
    platforms[platform] = {
      signature,
      url: `${releaseDownloadBase}/${encodeURIComponent(installer.name)}`,
    }
  }
  return platforms
}

function createUpdaterManifest(tag, notes, platforms) {
  return JSON.stringify(
    {
      version: tag.replace(/^v/, ""),
      notes,
      pub_date: new Date().toISOString(),
      platforms,
    },
    null,
    2
  )
}

async function replaceUpdaterAsset({
  github,
  owner,
  repo,
  releaseId,
  assets,
  data,
}) {
  const existing = findSingleNamedAsset({
    assets,
    name: UPDATER_FILENAME,
    required: false,
  })
  return replaceReleaseAsset({
    github,
    owner,
    repo,
    releaseId,
    name: UPDATER_FILENAME,
    data,
    existing,
    assertReady: assertAssetReady,
    verifyReplacement: async (candidateId) => {
      const refreshedAssets = await listReleaseAssets({
        github,
        owner,
        repo,
        releaseId,
      })
      const uploaded = findSingleNamedAsset({
        assets: refreshedAssets,
        name: UPDATER_FILENAME,
        required: true,
        label: `uploaded ${UPDATER_FILENAME}`,
      })
      if (uploaded.id !== candidateId) {
        throw new Error(`Uploaded ${UPDATER_FILENAME} identity changed`)
      }
      assertAssetReady(uploaded, `Uploaded ${UPDATER_FILENAME}`)
    },
  })
}

async function assertPublishableReleaseAssets({
  github,
  context,
  core,
  releaseId,
}) {
  const { owner, repo } = context.repo
  const assets = await listReleaseAssets({ github, owner, repo, releaseId })
  assertNoLegacyMcpAssets(assets)
  replaceReleaseAsset.assertNoReplacementResidue(assets, UPDATER_FILENAME)
  await resolveUpdaterPlatforms({ github, owner, repo, assets, core })
  const updater = findSingleNamedAsset({
    assets,
    name: UPDATER_FILENAME,
    required: true,
  })
  assertAssetReady(updater, UPDATER_FILENAME)
}

async function uploadUpdaterJson({
  github,
  context,
  core,
  tag,
  releaseId,
  notes,
}) {
  const { owner, repo } = context.repo
  const releaseDownloadBase =
    `https://github.com/${owner}/${repo}/releases/download/` +
    encodeURIComponent(tag)

  const assets = await listReleaseAssets({ github, owner, repo, releaseId })
  assertNoLegacyMcpAssets(assets)
  const resolvedPlatforms = await resolveUpdaterPlatforms({
    github,
    owner,
    repo,
    assets,
    core,
  })
  const platforms = formatUpdaterPlatforms(
    resolvedPlatforms,
    releaseDownloadBase
  )
  const manifest = createUpdaterManifest(tag, notes, platforms)
  const replaced = await replaceUpdaterAsset({
    github,
    owner,
    repo,
    releaseId,
    assets,
    data: manifest,
  })
  if (replaced) {
    core.info(`Deleted existing ${UPDATER_FILENAME} asset`)
  }
  core.info(
    `Uploaded ${UPDATER_FILENAME} for ${tag} ` +
      `(${Object.keys(platforms).length} platforms)`
  )
}

module.exports = uploadUpdaterJson
module.exports.assertPublishableReleaseAssets = assertPublishableReleaseAssets
