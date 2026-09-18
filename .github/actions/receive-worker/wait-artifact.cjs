const POLL_INTERVAL_MS = 30_000
const TIMEOUT_MS = 90 * 60_000
const PAGE_SIZE = 100

async function findArtifact({ github, context, name }) {
  const artifacts = await github.paginate(
    github.rest.actions.listWorkflowRunArtifacts,
    { ...context.repo, run_id: context.runId, per_page: PAGE_SIZE }
  )
  const matches = artifacts.filter(
    (item) => item.name === name && !item.expired
  )
  if (matches.length > 1) throw new Error(`ambiguous worker artifact: ${name}`)
  return matches[0]
}

async function producerConclusion({ github, context, target }) {
  const jobs = await github.paginate(
    github.rest.actions.listJobsForWorkflowRun,
    {
      ...context.repo,
      run_id: context.runId,
      filter: "latest",
      per_page: PAGE_SIZE,
    }
  )
  const producer = jobs.find(
    (job) =>
      job.name === `Worker ${target}` || job.name.endsWith(`/ Worker ${target}`)
  )
  return producer?.status === "completed" ? producer.conclusion : null
}

module.exports = async function waitForArtifact(options) {
  const { core, name } = options
  const started = Date.now()
  core.info(`Checking independently compiled worker: ${name}`)
  while (Date.now() - started < TIMEOUT_MS) {
    // 先读生产者状态，再读产物，避免上传完成与 job 结束之间的竞态。
    const conclusion = await producerConclusion(options)
    const artifact = await findArtifact(options)
    if (artifact) {
      core.setOutput("id", String(artifact.id))
      core.info(
        `Worker ready after ${Math.round((Date.now() - started) / 1000)}s`
      )
      return
    }
    if (conclusion)
      throw new Error(`worker producer ended (${conclusion}) without ${name}`)
    if (options.checkOnly) {
      core.info("Worker is still running; application can compile in parallel")
      return
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS))
  }
  throw new Error(`timed out waiting for worker artifact: ${name}`)
}
