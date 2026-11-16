/* eslint-disable @typescript-eslint/no-require-imports */
const WINDOWS_TARGETS = ["x86_64-win7-windows-msvc", "i686-win7-windows-msvc"]
const LINUX_JOBS = ["Linux x64", "Linux arm64"]
const MAX_REPAIR_RUNS = WINDOWS_TARGETS.length + LINUX_JOBS.length

async function windowsJobs({ github, repository, runId, jobs }) {
  const build = jobs.find((job) => job.name === "repair / build")
  const sign = jobs.find((job) => job.name === "repair / sign")
  if (build?.conclusion !== "success" || sign?.conclusion !== "success")
    throw new Error(`Win7 repair ${runId} has not completed build and signing`)
  const artifacts = await github.paginate(
    github.rest.actions.listWorkflowRunArtifacts,
    { ...repository, run_id: runId, per_page: 100 }
  )
  const targets = WINDOWS_TARGETS.filter((target) =>
    artifacts.some((artifact) =>
      artifact.name === `windows-installer-verification-${target}` && !artifact.expired
    )
  )
  if (targets.length !== 1)
    throw new Error(`Win7 repair ${runId} must identify one signed target`)
  return {
    build,
    names: ["build", "sign"].map(
      (kind) => `build-windows7 (${targets[0]}) / ${kind}`
    ),
  }
}

function linuxJobs(runId, jobs) {
  const selected = LINUX_JOBS.flatMap((name) => {
    const build = jobs.find((job) => job.name === `repair / Build ${name}`)
    return build?.conclusion === "success" ? [{ build, name }] : []
  })
  if (selected.length !== 1)
    throw new Error(`Linux repair ${runId} must complete one matching target`)
  return {
    build: selected[0].build,
    names: [`build-tauri / Build ${selected[0].name}`],
  }
}

async function verifySourceLog(input, jobId) {
  const response = await input.github.request(
    "GET /repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
    { ...input.repository, job_id: jobId }
  )
  const log = typeof response.data === "string"
    ? response.data
    : Buffer.from(response.data).toString("utf8")
  const context = `[release-context] ${input.tag} source=${input.sourceRef} draft=${input.releaseId}`
  if (!log.includes(context))
    throw new Error(`Repair job ${jobId} does not verify the expected release source`)
}

module.exports = async function verifiedRepairJobs(input) {
  const ids = (input.runIds || "").split(",").map((id) => id.trim()).filter(Boolean)
  if (ids.length > MAX_REPAIR_RUNS || new Set(ids).size !== ids.length)
    throw new Error("Too many or duplicate repair runs")
  const verified = new Set()
  for (const id of ids) {
    if (!/^\d+$/.test(id) || !Number.isSafeInteger(Number(id)) || Number(id) <= 0)
      throw new Error("Invalid repair run ID")
    const runId = Number(id)
    const { data: run } = await input.github.rest.actions.getWorkflowRun({
      ...input.repository, run_id: runId,
    })
    if (
      run.head_branch !== "main" ||
      run.event !== "workflow_dispatch" ||
      run.head_repository?.full_name !== `${input.repository.owner}/${input.repository.repo}` ||
      run.conclusion !== "success"
    )
      throw new Error(`Repair ${id} has not succeeded on main`)
    const jobs = await input.github.paginate(
      input.github.rest.actions.listJobsForWorkflowRun,
      { ...input.repository, run_id: runId, per_page: 100 }
    )
    let selected
    if (run.path === ".github/workflows/release-repair-win7.yml")
      selected = await windowsJobs({
        github: input.github, repository: input.repository, runId, jobs,
      })
    else if (run.path === ".github/workflows/release-repair-linux.yml")
      selected = linuxJobs(runId, jobs)
    else throw new Error(`Unexpected repair workflow for ${id}`)
    await verifySourceLog(input, selected.build.id)
    for (const name of selected.names) {
      if (verified.has(name)) throw new Error(`Multiple repairs selected for ${name}`)
      verified.add(name)
    }
  }
  return verified
}
