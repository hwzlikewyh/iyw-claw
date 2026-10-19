/* eslint-disable @typescript-eslint/no-require-imports -- github-script 使用 CommonJS 扩展点。 */
const {
  resolveCommitSha,
  findRelease,
  assertDraftContext,
} = require("./release-context.cjs")

async function loadReleaseBody(github, owner, repo, commitSha) {
  const { data: commit } = await github.rest.repos.getCommit({
    owner,
    repo,
    ref: commitSha,
  })
  return commit.commit.message?.trim() || "_No commit message._"
}

async function updateDraft(github, owner, repo, release, desired) {
  if (!release.draft) {
    throw new Error(
      `Release for tag ${desired.tag} already exists and is not a draft.`
    )
  }
  const unchanged =
    release.prerelease === desired.prerelease &&
    release.name === desired.name &&
    (release.body ?? "").trim() === desired.body
  if (unchanged) return release

  const { data } = await github.rest.repos.updateRelease({
    owner,
    repo,
    release_id: release.id,
    name: desired.name,
    prerelease: desired.prerelease,
    body: desired.body,
  })
  return data
}

async function createOrReuseDraft({
  github,
  context,
  core,
  tag,
  prerelease,
  sourceRef,
}) {
  const { owner, repo } = context.repo
  const commitSha = await resolveCommitSha(github, context.repo, tag)
  if (commitSha !== sourceRef)
    throw new Error(
      `Release source mismatch: ${tag}=${commitSha}, build=${sourceRef}`
    )
  const body = await loadReleaseBody(github, owner, repo, commitSha)
  const desired = { tag, prerelease, name: `iyw-claw ${tag}`, body }
  let release

  const existing = await findRelease(github, context.repo, tag)
  if (existing) {
    release = await updateDraft(github, owner, repo, existing, desired)
    core.info(`Reusing existing draft release #${release.id}`)
  } else {
    const created = await github.rest.repos.createRelease({
      owner,
      repo,
      tag_name: tag,
      target_commitish: commitSha,
      name: desired.name,
      body,
      draft: true,
      prerelease,
    })
    release = created.data
    core.info(`Created draft release #${release.id}`)
  }

  await validateDraft({ github, context, tag, release, sourceRef })
  core.setOutput("release_id", String(release.id))
  core.setOutput("release_url", release.html_url)
  core.setOutput("release_body", body)
}

async function validateDraft({ github, context, tag, release, sourceRef }) {
  await assertDraftContext({
    github,
    repository: context.repo,
    tag,
    releaseId: release.id,
    sourceRef,
  })
}

module.exports = createOrReuseDraft
