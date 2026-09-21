const PAGE_SIZE = 100
const TRUSTED_EVENTS = new Set(["push", "workflow_dispatch"])

function matchesArtifact(artifact, repository, key) {
  const run = artifact.workflow_run
  return (
    artifact.name === key &&
    !artifact.expired &&
    Date.parse(artifact.expires_at) > Date.now() &&
    run?.repository_id === repository.id &&
    run.head_repository_id === repository.id &&
    run.head_branch === repository.default_branch
  )
}

async function findArtifact({ github, context, core, key }) {
  core.setOutput("artifact-id", "")
  core.setOutput("run-id", "")
  try {
    const repository = context.payload.repository
    const artifacts = await github.paginate(
      github.rest.actions.listArtifactsForRepo,
      { ...context.repo, name: key, per_page: PAGE_SIZE }
    )
    const candidates = artifacts
      .filter((artifact) => matchesArtifact(artifact, repository, key))
      .sort((left, right) => right.id - left.id)
    for (const artifact of candidates) {
      const { data: run } = await github.rest.actions.getWorkflowRun({
        ...context.repo,
        run_id: artifact.workflow_run.id,
      })
      if (
        !TRUSTED_EVENTS.has(run.event) ||
        run.head_repository?.id !== repository.id ||
        run.head_branch !== repository.default_branch
      )
        continue
      // 成品在 worker 校验后立即上传，不依赖后续应用打包或签名的结果。
      core.setOutput("artifact-id", String(artifact.id))
      core.setOutput("run-id", String(run.id))
      core.info(
        `Worker artifact backup found: ${artifact.id} from run ${run.id}`
      )
      return
    }
    core.info(
      `No worker artifact backup for ${key}; using cache or Cargo build`
    )
  } catch (error) {
    core.warning(`Worker artifact lookup failed: ${error.message}`)
  }
}

module.exports = findArtifact
