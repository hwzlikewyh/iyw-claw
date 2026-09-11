import { createInterface } from "node:readline/promises"
import { mkdirSync, writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"
import { gh, github } from "../.github/scripts/release/github-assets.mjs"
import {
  assertPolicy,
  connectFusion,
  publishFusion,
  releasePolicy,
} from "./release-fusion.mjs"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const REPO = "hwzlikewyh/iyw-claw"
const POLL_MS = 30_000
const DISCOVERY_ATTEMPTS = 10

async function readVersion(args) {
  if (args[0] && !args[0].startsWith("--")) return args[0]
  const input = createInterface({
    input: process.stdin,
    output: process.stdout,
  })
  try {
    return (await input.question("发布版本号（例如 0.1.194）：")).trim()
  } finally {
    input.close()
  }
}

function findRelease(version) {
  const releases = github(`repos/${REPO}/releases?per_page=100`)
  return releases.find((item) => item.tag_name === `v${version}` && !item.draft)
}

function versionRuns(version) {
  return JSON.parse(
    gh([
      "run",
      "list",
      "--repo",
      REPO,
      "--workflow",
      "release.yml",
      "--limit",
      "100",
      "--json",
      "databaseId,displayTitle,status,conclusion,event",
    ])
  ).filter((run) => run.displayTitle === `Release v${version}`)
}

async function startOrResume(version) {
  const runs = versionRuns(version)
  const active = runs.find((run) => run.status !== "completed")
  if (active) return active.databaseId
  if (
    runs[0]?.conclusion === "failure" ||
    runs[0]?.conclusion === "cancelled"
  ) {
    gh(
      ["run", "rerun", String(runs[0].databaseId), "--failed", "--repo", REPO],
      { stdio: "inherit" }
    )
    return runs[0].databaseId
  }
  const oldIds = new Set(runs.map((run) => run.databaseId))
  const text = gh([
    "workflow",
    "run",
    "release.yml",
    "--repo",
    REPO,
    "--ref",
    "main",
    "-f",
    `version=${version}`,
  ])
  const link = text
    .trim()
    .split(/\s+/)
    .find((value) =>
      value.startsWith(`https://github.com/${REPO}/actions/runs/`)
    )
  if (link) return new URL(link).pathname.split("/").pop()
  for (let attempt = 0; attempt < DISCOVERY_ATTEMPTS; attempt++) {
    const created = versionRuns(version).find(
      (run) => !oldIds.has(run.databaseId)
    )
    if (created) return created.databaseId
    await new Promise((done) => setTimeout(done, POLL_MS))
  }
  throw new Error(
    "workflow dispatched but run lookup timed out; run the same version again"
  )
}

async function waitForRelease(version, runId) {
  console.log(`[release] https://github.com/${REPO}/actions/runs/${runId}`)
  let lastStatus = ""
  // 成功发布后不等待非必需的 Linux ARM64 后续任务。
  while (!findRelease(version)) {
    const run = github(`repos/${REPO}/actions/runs/${runId}`)
    if (run.status === "completed" && run.conclusion !== "success")
      throw new Error(
        `build ${runId} ${run.conclusion}; retry the same version after addressing the failed step`
      )
    if (run.status === "completed")
      throw new Error("workflow finished without a published release")
    const jobs = github(`repos/${REPO}/actions/runs/${runId}/jobs?per_page=100`)
    const status = jobs.jobs
      .filter((job) => job.status !== "completed")
      .map((job) => `${job.name}: ${job.status}`)
      .join("; ")
    if (status !== lastStatus) {
      console.log(`[release] ${status || run.status}`)
      lastStatus = status
    }
    await new Promise((done) => setTimeout(done, POLL_MS))
  }
}

async function main() {
  const args = process.argv.slice(2)
  if (args.includes("--help")) {
    console.log(
      "node scripts/release-desktop.mjs [x.y.z] [--check]\n输入版本后自动构建、签名并发布 Fusion，默认 1% 灰度。--check 仅检查环境和登录。"
    )
    return
  }
  const version = await readVersion(args)
  if (Number(process.versions.node.split(".")[0]) < 22)
    throw new Error("Node.js 22 or newer is required")
  releasePolicy(version, "")
  const unexpected = args.filter((arg) => arg !== version && arg !== "--check")
  if (unexpected.length)
    throw new Error(`unknown arguments: ${unexpected.join(" ")}`)
  gh(["auth", "status"], { stdio: "ignore" })
  const api = await connectFusion()
  const existing = await api(
    `/admin/api/app-releases?q=${encodeURIComponent(version)}&pageSize=100`
  )
  for (const item of existing.items.filter(
    (item) =>
      item.version === version &&
      item.productKey === "iyw-claw" &&
      item.channel === "stable"
  ))
    assertPolicy(item, version)
  try {
    github(
      `repos/${REPO}/contents/.github/workflows/release-windows-staged.yml?ref=main`
    )
  } catch {
    throw new Error(
      "新的构建流程尚未部署到 GitHub main，请先推送本次改动，或检查 GitHub 访问权限。"
    )
  }
  console.log(
    `[release] ${version}: Windows x64/x86, macOS x64/ARM64, Linux x64; Fusion 1% optional rollout`
  )
  if (args.includes("--check")) return
  if (!findRelease(version))
    await waitForRelease(version, await startOrResume(version))
  const directory = join(ROOT, "artifacts", `release-${version}`)
  mkdirSync(directory, { recursive: true })
  const result = await publishFusion({ repo: REPO, version, directory })
  writeFileSync(
    join(directory, "fusion-release.json"),
    JSON.stringify(result, null, 2)
  )
}

main().catch((error) => {
  console.error(`[release] ${error.message}`)
  process.exitCode = 1
})
