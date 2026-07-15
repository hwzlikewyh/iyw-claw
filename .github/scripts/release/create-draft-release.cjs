async function resolveCommitSha(github, owner, repo, tag) {
  const { data: tagRef } = await github.rest.git.getRef({
    owner,
    repo,
    ref: `tags/${tag}`,
  })
  if (tagRef.object.type !== "tag") return tagRef.object.sha

  const { data: annotatedTag } = await github.rest.git.getTag({
    owner,
    repo,
    tag_sha: tagRef.object.sha,
  })
  if (annotatedTag.object.type !== "commit") {
    throw new Error(
      `Tag ${tag} points to ${annotatedTag.object.type}, not a commit.`
    )
  }
  return annotatedTag.object.sha
}

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

async function createOrReuseDraft({ github, context, core, tag, prerelease }) {
  const { owner, repo } = context.repo
  const commitSha = await resolveCommitSha(github, owner, repo, tag)
  const body = await loadReleaseBody(github, owner, repo, commitSha)
  const desired = { tag, prerelease, name: `iyw-claw ${tag}`, body }
  let release

  try {
    const existing = await github.rest.repos.getReleaseByTag({
      owner,
      repo,
      tag,
    })
    release = await updateDraft(github, owner, repo, existing.data, desired)
    core.info(`Reusing existing draft release #${release.id}`)
  } catch (error) {
    if (error.status !== 404) throw error
    const created = await github.rest.repos.createRelease({
      owner,
      repo,
      tag_name: tag,
      name: desired.name,
      body,
      draft: true,
      prerelease,
    })
    release = created.data
    core.info(`Created draft release #${release.id}`)
  }

  core.setOutput("release_id", String(release.id))
  core.setOutput("release_url", release.html_url)
  core.setOutput("release_body", body)
}

module.exports = createOrReuseDraft
