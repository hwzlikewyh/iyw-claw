import { spawnSync } from "node:child_process"
import { appendFileSync, existsSync, readdirSync } from "node:fs"
import process from "node:process"

const STATS_TIMEOUT_MS = 15_000
const result = spawnSync("sccache", ["--show-stats"], {
  encoding: "utf8",
  timeout: STATS_TIMEOUT_MS,
  windowsHide: true,
})
if (result.error || result.status !== 0)
  throw new Error(
    `compiler cache statistics failed: ${result.error?.code || result.status}`
  )
const stats = result.stdout
process.stdout.write(stats)
const readCount = (label) =>
  Number(stats.match(new RegExp(`^${label} +([0-9]+)`, "m"))?.[1] || 0)
const hits = readCount("Cache hits")
const misses = readCount("Cache misses")
const writeErrors = readCount("Cache write errors")
const readErrors = readCount("Cache read errors")
if (writeErrors || readErrors)
  console.warn(
    `::warning::Compiler cache read errors=${readErrors}, write errors=${writeErrors}; compilation results still require normal verification`
  )
const directory = process.env.SCCACHE_DIR
const save = Boolean(
  process.env.IYW_SCCACHE_CACHE_KEY &&
  misses > 0 &&
  directory &&
  existsSync(directory) &&
  readdirSync(directory).length > 0
)
appendFileSync(process.env.GITHUB_OUTPUT, `save=${save}\n`)
if (process.env.GITHUB_STEP_SUMMARY)
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    `\nCompiler cache: hits ${hits}, misses ${misses}, read errors ${readErrors}, write errors ${writeErrors}. Local object cache save requested: ${save}.\n`
  )
