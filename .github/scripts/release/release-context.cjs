/* eslint-disable @typescript-eslint/no-require-imports -- github-script 使用 CommonJS 扩展点。 */
const { execFileSync } = require("node:child_process")
const { readFileSync } = require("node:fs")
const MAX_DIAGNOSTIC_TAG_LENGTH = 128

async function resolveCommitSha(github, repository, tag) {
  const { data } = await github.rest.git.getRef({
    ...repository,
    ref: `tags/${tag}`,
  })
  if (data.object.type === "commit") return data.object.sha
  if (data.object.type !== "tag") throw new Error(`Invalid release tag ${tag}`)
  const { data: annotated } = await github.rest.git.getTag({
    ...repository,
    tag_sha: data.object.sha,
  })
  if (annotated.object.type !== "commit")
    throw new Error(`Tag ${tag} does not point to a commit`)
  return annotated.object.sha
}

async function findRelease(github, repository, tag) {
  try {
    return (await github.rest.repos.getReleaseByTag({ ...repository, tag }))
      .data
  } catch (error) {
    if (error.status !== 404) throw error
    const releases = await github.paginate(github.rest.repos.listReleases, {
      ...repository,
      per_page: 100,
    })
    const matching = releases.filter((release) => release.tag_name === tag)
    if (matching.length > 1)
      throw new Error(`Multiple releases exist for ${tag}`)
    return matching[0]
  }
}

function releaseDetails(release) {
  return JSON.stringify({
    type: Array.isArray(release) ? "array" : typeof release,
    id: typeof release?.id === "number" ? release.id : null,
    tag:
      typeof release?.tag_name === "string"
        ? release.tag_name.slice(0, MAX_DIAGNOSTIC_TAG_LENGTH)
        : null,
    draft: typeof release?.draft === "boolean" ? release.draft : null,
  })
}

async function assertDraftContext({
  github,
  repository,
  tag,
  releaseId,
  sourceRef,
}) {
  if (!/^\d+$/.test(String(releaseId)) || !/^[a-f0-9]{40}$/.test(sourceRef))
    throw new Error("Expected a release ID and fixed source commit")
  let release
  try {
    release = (
      await github.rest.repos.getRelease({
        ...repository,
        release_id: Number(releaseId),
      })
    ).data
  } catch (error) {
    if (error.status !== 404) throw error
    throw new Error(
      `Release #${releaseId} for ${tag} is missing or inaccessible. ` +
        "Run the complete Release workflow to obtain a new shared release ID; " +
        "do not rerun only failed jobs or recreate drafts during a build.",
      { cause: error }
    )
  }
  if (
    release?.draft !== true ||
    release.tag_name !== tag ||
    String(release.id) !== String(releaseId)
  )
    throw new Error(
      `Release #${releaseId} must be the draft for ${tag}; received=${releaseDetails(release)}`
    )
  const tagCommit = await resolveCommitSha(github, repository, tag)
  if (tagCommit !== sourceRef)
    throw new Error(
      `Release source mismatch: ${tag}=${tagCommit}, build=${sourceRef}`
    )
  return release
}

async function assertBuildContext(input) {
  const actual = execFileSync("git", ["rev-parse", "HEAD"], {
    encoding: "utf8",
    windowsHide: true,
  }).trim()
  if (input.sourceRef && input.sourceRef !== actual)
    throw new Error(
      `Build checkout mismatch: expected=${input.sourceRef}, actual=${actual}`
    )
  const { version } = JSON.parse(readFileSync("package.json", "utf8"))
  if (input.tag !== `v${version}`)
    throw new Error(
      `Release version mismatch: ${input.tag}, package=${version}`
    )
  const release = await assertDraftContext({ ...input, sourceRef: actual })
  console.log(
    `[release-context] ${input.tag} source=${actual} draft=${release.id}`
  )
  return release
}

module.exports = {
  resolveCommitSha,
  findRelease,
  assertDraftContext,
  assertBuildContext,
}
