import { execFileSync } from "node:child_process"
import { appendFileSync } from "node:fs"

const CHECK_TIMEOUT_MS = 10_000
let wrapper = ""
if (process.env.SCCACHE_INSTALL_OUTCOME === "success") {
  try {
    const version = execFileSync("sccache", ["--version"], {
      encoding: "utf8",
      timeout: CHECK_TIMEOUT_MS,
      windowsHide: true,
    }).trim()
    wrapper = "sccache"
    console.log(`[compiler-cache] enabled: ${version}`)
  } catch (error) {
    console.warn(
      `[compiler-cache] executable check failed: ${error.code || error.status}`
    )
  }
}
if (!wrapper)
  console.warn(
    "::warning::sccache unavailable; continuing with ordinary Cargo compilation"
  )
// 空字符串覆盖 job 级别的 wrapper，防止 Cargo 调用不存在的 sccache。
appendFileSync(process.env.GITHUB_ENV, `RUSTC_WRAPPER=${wrapper}\n`)
