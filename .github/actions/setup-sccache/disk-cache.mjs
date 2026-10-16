import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { appendFileSync, mkdirSync } from "node:fs"
import { join } from "node:path"
import process from "node:process"

const COMPILER_TIMEOUT_MS = 10_000
const CACHE_SIZE = "1G"

export function configureDiskCache(target) {
  if (!/^[a-z0-9_-]+$/i.test(target))
    throw new Error("invalid compiler cache target")
  const compiler = execFileSync("rustc", ["-vV"], {
    encoding: "utf8",
    timeout: COMPILER_TIMEOUT_MS,
    windowsHide: true,
  })
  const identity = createHash("sha256")
    .update(compiler)
    .digest("hex")
    .slice(0, 16)
  const env = process.env
  const directory = join(env.RUNNER_TEMP, `desktop-sccache-${target}`)
  const prefix = `v1-desktop-objects-${env.RUNNER_OS}-${env.RUNNER_ARCH}-${target}-${identity}-`
  const key = `${prefix}${env.GITHUB_SHA}-${env.GITHUB_RUN_ID}-${env.GITHUB_RUN_ATTEMPT}`
  mkdirSync(directory, { recursive: true })
  // 只扩大内容寻址对象缓存的恢复范围；Cargo target 仍绑定精确补丁身份。
  appendFileSync(
    env.GITHUB_ENV,
    [
      "SCCACHE_GHA_ENABLED=false",
      `SCCACHE_DIR=${directory}`,
      `SCCACHE_CACHE_SIZE=${CACHE_SIZE}`,
      `IYW_SCCACHE_CACHE_KEY=${key}`,
      `IYW_SCCACHE_CACHE_PREFIX=${prefix}`,
      "",
    ].join("\n")
  )
  console.log(`[compiler-cache] local objects: ${CACHE_SIZE}; prefix=${prefix}`)
}
