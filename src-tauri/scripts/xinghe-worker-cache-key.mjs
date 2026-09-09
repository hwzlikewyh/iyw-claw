import { execFileSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import { fileURLToPath } from "node:url"

const ROOT = fileURLToPath(new URL("../..", import.meta.url))
const INPUTS = [
  "harness/xinghe-worker",
  "harness/codex",
  "src-tauri/vendor/sacp-tokio",
  ".cargo",
  "rust-toolchain",
  "rust-toolchain.toml",
  "src-tauri/scripts/xinghe-worker-build.mjs",
]
const COMPILER_ENV =
  /^(CARGO_PROFILE_|CARGO_TARGET_|CARGO_ENCODED_RUSTFLAGS$|RUSTFLAGS$|CC($|_)|CXX($|_)|CFLAGS($|_)|CXXFLAGS($|_)|AR($|_)|RANLIB($|_)|CMAKE_|MACOSX_DEPLOYMENT_TARGET$|SDKROOT$|VCToolsVersion$|WindowsSDKVersion$|ImageOS$|ImageVersion$)/i

export function workerCacheKey(target, root = ROOT, environment = process.env) {
  if (!/^[a-z0-9_]+-(?:[a-z0-9_]+-)*[a-z0-9_]+$/.test(target)) {
    throw new Error("invalid worker cache target")
  }
  const options = { cwd: root, encoding: "utf8", windowsHide: true }
  const files = execFileSync(
    "git",
    [
      "ls-files",
      "-z",
      "--cached",
      "--others",
      "--exclude-standard",
      "--",
      ...INPUTS,
    ],
    options
  )
    .split("\0")
    .filter(Boolean)
  const hash = createHash("sha256")
  const compiler = execFileSync(
    "rustc",
    ["--version", "--verbose"],
    options
  ).trim()
  const configuration = Object.entries(environment)
    .filter(([name]) => COMPILER_ENV.test(name))
    .sort(([left], [right]) => left.localeCompare(right))
  hash.update(JSON.stringify({ compiler, target, configuration }))
  // 只缓存编译输入，打包脚本和应用版本不触发引擎重编。
  for (const path of [...new Set(files)].sort()) {
    if (!existsSync(resolve(root, path))) continue
    hash
      .update(path)
      .update("\0")
      .update(readFileSync(resolve(root, path)))
      .update("\0")
  }
  return `xinghe-build-v2-${target}-${hash.digest("hex")}`
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  console.log(workerCacheKey(process.argv[2] || ""))
}
