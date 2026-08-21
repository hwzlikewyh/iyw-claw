const UPDATER_FILENAME = "latest.json"
const LEGACY_MCP_ASSET_PATTERN = /^iyw-claw-mcp(?:-|\.|$)/i

// Match installers by suffix: the product name is non-ASCII ("原助理") and
// gets sanitized away in uploaded asset names, so never match on the prefix.
const PLATFORM_PATTERNS = [
  { platform: "windows-x86_64", pattern: /x64-setup\.exe$/ },
  { platform: "windows-i686", pattern: /x86-setup\.exe$/ },
  { platform: "darwin-x86_64", pattern: /x64\.app\.tar\.gz$/ },
  { platform: "darwin-aarch64", pattern: /aarch64\.app\.tar\.gz$/ },
  { platform: "linux-x86_64", pattern: /amd64\.AppImage$/ },
]

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

async function fetchAssetText(github, owner, repo, assetId) {
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

async function uploadUpdaterJson({
  github,
  context,
  core,
  tag,
  releaseId,
  notes,
}) {
  const { owner, repo } = context.repo
  const version = tag.replace(/^v/, "")
  const releaseDownloadBase =
    `https://github.com/${owner}/${repo}/releases/download/` +
    encodeURIComponent(tag)

  const assets = await github.paginate(github.rest.repos.listReleaseAssets, {
    owner,
    repo,
    release_id: releaseId,
    per_page: 100,
  })

  assertNoLegacyMcpAssets(assets)

  const platforms = {}
  for (const { platform, pattern } of PLATFORM_PATTERNS) {
    const installer = assets.find((asset) => pattern.test(asset.name))
    if (!installer) {
      core.warning(`No installer asset matching ${pattern} for ${platform}`)
      continue
    }
    const signature = assets.find(
      (asset) => asset.name === `${installer.name}.sig`
    )
    if (!signature) {
      core.warning(`Missing signature for ${installer.name} (${platform})`)
      continue
    }
    platforms[platform] = {
      signature: await fetchAssetText(github, owner, repo, signature.id),
      url: `${releaseDownloadBase}/${encodeURIComponent(installer.name)}`,
    }
    core.info(`${platform}: ${installer.name}`)
  }

  if (Object.keys(platforms).length === 0) {
    throw new Error(
      "No updater platforms matched release assets; " +
        "refusing to publish without a valid latest.json"
    )
  }

  const manifest = JSON.stringify(
    {
      version,
      notes,
      pub_date: new Date().toISOString(),
      platforms,
    },
    null,
    2
  )

  const existing = assets.find((asset) => asset.name === UPDATER_FILENAME)
  if (existing) {
    await github.rest.repos.deleteReleaseAsset({
      owner,
      repo,
      asset_id: existing.id,
    })
    core.info(`Deleted existing ${UPDATER_FILENAME} asset`)
  }

  await github.rest.repos.uploadReleaseAsset({
    owner,
    repo,
    release_id: releaseId,
    name: UPDATER_FILENAME,
    data: manifest,
    headers: {
      "content-type": "application/json",
      "content-length": Buffer.byteLength(manifest),
    },
  })
  core.info(
    `Uploaded ${UPDATER_FILENAME} for ${tag} ` +
      `(${Object.keys(platforms).length} platforms)`
  )
}

module.exports = uploadUpdaterJson
